use std::{
    fs,
    os::{fd::FromRawFd, unix::net::UnixListener as StdUnixListener},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, anyhow};
use futures_util::SinkExt;
use indoc::concatdoc;
use sqlx::{MySqlPool, mysql::MySqlConnectOptions, prelude::*};
use tokio::{net::UnixListener as TokioUnixListener, task::JoinHandle, time::interval};
use tokio_util::task::TaskTracker;
// use tokio_util::sync::CancellationToken;

use crate::{
    core::{
        common::UnixUser,
        protocol::{Response, create_server_to_client_message_stream},
    },
    server::{
        config::{MysqlConfig, ServerConfig},
        session_handler::session_handler,
    },
};

// TODO: implement graceful shutdown and graceful restarts
#[allow(dead_code)]
pub struct Supervisor {
    config: ServerConfig,
    systemd_mode: bool,

    // sighup_cancel_token: CancellationToken,
    // sigterm_cancel_token: CancellationToken,
    // signal_handler_task: JoinHandle<()>,
    db_connection_pool: MySqlPool,
    // listener: TokioUnixListener,
    listener_task: JoinHandle<anyhow::Result<()>>,
    handler_task_tracker: TaskTracker,

    watchdog_timeout: Option<Duration>,
    systemd_watchdog_task: Option<JoinHandle<()>>,

    connection_counter: std::sync::Arc<()>,
    status_notifier_task: Option<JoinHandle<()>>,
}

impl Supervisor {
    pub async fn new(config: ServerConfig, systemd_mode: bool) -> anyhow::Result<Self> {
        let mut watchdog_duration = None;
        let mut watchdog_micro_seconds = 0;
        let watchdog_task =
            if systemd_mode && sd_notify::watchdog_enabled(true, &mut watchdog_micro_seconds) {
                watchdog_duration = Some(Duration::from_micros(watchdog_micro_seconds));
                log::debug!(
                    "Systemd watchdog enabled with {} millisecond interval",
                    watchdog_micro_seconds.div_ceil(1000),
                );
                Some(spawn_watchdog_task(watchdog_duration.unwrap()))
            } else {
                log::debug!("Systemd watchdog not enabled, skipping watchdog thread");
                None
            };

        let db_connection_pool = create_db_connection_pool(&config.mysql).await?;

        let connection_counter = Arc::new(());
        let status_notifier_task = if systemd_mode {
            Some(spawn_status_notifier_task(connection_counter.clone()))
        } else {
            None
        };

        // TODO: try to detech systemd socket before using the provided socket path
        let listener = match config.socket_path {
            Some(ref path) => create_unix_listener_with_socket_path(path.clone()).await?,
            None => create_unix_listener_with_systemd_socket().await?,
        };

        let listener_task = {
            let connection_counter = connection_counter.clone();
            let config_clone = config.clone();
            tokio::spawn(spawn_listener_task(
                listener,
                config_clone,
                connection_counter,
            ))
        };

        // let sighup_cancel_token = CancellationToken::new();
        // let sigterm_cancel_token = CancellationToken::new();

        Ok(Self {
            config,
            systemd_mode,
            // sighup_cancel_token,
            // sigterm_cancel_token,
            // signal_handler_task,
            db_connection_pool,
            // listener,
            listener_task,
            handler_task_tracker: TaskTracker::new(),
            watchdog_timeout: watchdog_duration,
            systemd_watchdog_task: watchdog_task,
            connection_counter,
            status_notifier_task,
        })
    }

    pub async fn run(self) -> anyhow::Result<()> {
        self.listener_task.await?
    }
}

fn spawn_watchdog_task(duration: Duration) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = interval(duration.div_f32(2.0));
        log::debug!(
            "Starting systemd watchdog task, pinging every {} milliseconds",
            duration.div_f32(2.0).as_millis()
        );
        loop {
            interval.tick().await;
            if let Err(err) = sd_notify::notify(false, &[sd_notify::NotifyState::Watchdog]) {
                log::warn!("Failed to notify systemd watchdog: {}", err);
            } else {
                log::trace!("Ping sent to systemd watchdog");
            }
        }
    })
}

fn spawn_status_notifier_task(connection_counter: std::sync::Arc<()>) -> JoinHandle<()> {
    const NON_CONNECTION_ARC_COUNT: usize = 4;
    const STATUS_UPDATE_INTERVAL_SECS: Duration = Duration::from_secs(1);

    tokio::spawn(async move {
        let mut interval = interval(STATUS_UPDATE_INTERVAL_SECS);
        loop {
            interval.tick().await;
            log::trace!("Updating systemd status notification");
            let count = Arc::strong_count(&connection_counter) - NON_CONNECTION_ARC_COUNT;
            let message = if count > 0 {
                format!("Handling {} connections", count)
            } else {
                "Waiting for connections".to_string()
            };
            sd_notify::notify(false, &[sd_notify::NotifyState::Status(message.as_str())]).ok();
        }
    })
}

async fn create_unix_listener_with_socket_path(
    socket_path: PathBuf,
) -> anyhow::Result<TokioUnixListener> {
    let parent_directory = socket_path.parent().unwrap();
    if !parent_directory.exists() {
        log::debug!("Creating directory {:?}", parent_directory);
        fs::create_dir_all(parent_directory)?;
    }

    log::info!("Listening on socket {:?}", socket_path);

    match fs::remove_file(socket_path.as_path()) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }

    let listener = TokioUnixListener::bind(socket_path)?;

    Ok(listener)
}

async fn create_unix_listener_with_systemd_socket() -> anyhow::Result<TokioUnixListener> {
    let fd = sd_notify::listen_fds()
        .context("Failed to get file descriptors from systemd")?
        .next()
        .context("No file descriptors received from systemd")?;

    debug_assert!(fd == 3, "Unexpected file descriptor from systemd: {}", fd);

    log::debug!(
        "Received file descriptor from systemd with id: '{}', assuming socket",
        fd
    );

    let std_unix_listener = unsafe { StdUnixListener::from_raw_fd(fd) };
    let listener = TokioUnixListener::from_std(std_unix_listener)?;
    Ok(listener)
}

async fn create_db_connection_pool(config: &MysqlConfig) -> anyhow::Result<MySqlPool> {
    let mut mysql_options = MySqlConnectOptions::new()
        .database("mysql")
        .log_statements(log::LevelFilter::Trace);

    if let Some(username) = config.username.as_ref() {
        mysql_options = mysql_options.username(username);
    }

    if let Some(password) = config.password.as_ref() {
        mysql_options = mysql_options.password(password);
    }

    if let Some(socket_path) = config.socket_path.as_ref() {
        mysql_options = mysql_options.socket(socket_path);
    } else if let Some(host) = config.host.as_ref() {
        mysql_options = mysql_options.host(host);
        mysql_options = mysql_options.port(config.port);
    } else {
        anyhow::bail!("No MySQL host or socket path provided");
    }

    match tokio::time::timeout(
        Duration::from_secs(config.timeout),
        MySqlPool::connect_with(mysql_options),
    )
    .await
    {
        Ok(connection) => connection.context("Failed to connect to the database"),
        Err(_) => Err(anyhow!("Timed out after {} seconds", config.timeout))
            .context("Failed to connect to the database"),
    }
}

// fn spawn_signal_handler_task(
//     sighup_token: CancellationToken,
//     sigterm_token: CancellationToken,
// ) -> JoinHandle<()> {
//     tokio::spawn(async move {
//         let mut sighup_stream = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
//             .expect("Failed to set up SIGHUP handler");
//         let mut sigterm_stream = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
//             .expect("Failed to set up SIGTERM handler");

//         loop {
//             tokio::select! {
//                 _ = sighup_stream.recv() => {
//                     log::info!("Received SIGHUP signal");
//                     sighup_token.cancel();
//                 }
//                 _ = sigterm_stream.recv() => {
//                     log::info!("Received SIGTERM signal");
//                     sigterm_token.cancel();
//                     break;
//                 }
//             }
//         }
//     })
// }

async fn spawn_listener_task(
    listener: TokioUnixListener,
    config: ServerConfig,
    connection_counter: Arc<()>,
) -> anyhow::Result<()> {
    sd_notify::notify(false, &[sd_notify::NotifyState::Ready])?;

    while let Ok((conn, _addr)) = listener.accept().await {
        log::debug!("Got new connection");

        let uid = match conn.peer_cred() {
            Ok(cred) => cred.uid(),
            Err(e) => {
                log::error!("Failed to get peer credentials from socket: {}", e);
                let mut message_stream = create_server_to_client_message_stream(conn);
                message_stream
                    .send(Response::Error(
                        (concatdoc! {
                            "Server failed to get peer credentials from socket\n",
                            "Please check the server logs or contact the system administrators"
                        })
                        .to_string(),
                    ))
                    .await
                    .ok();
                continue;
            }
        };

        log::debug!("Validated peer UID: {}", uid);

        let _connection_counter_guard = Arc::clone(&connection_counter);

        let unix_user = match UnixUser::from_uid(uid) {
            Ok(user) => user,
            Err(e) => {
                log::error!("Failed to get username from uid: {}", e);
                let mut message_stream = create_server_to_client_message_stream(conn);
                message_stream
                    .send(Response::Error(
                        (concatdoc! {
                            "Server failed to get user data from the system\n",
                            "Please check the server logs or contact the system administrators"
                        })
                        .to_string(),
                    ))
                    .await
                    .ok();
                continue;
            }
        };

        log::info!("Accepted connection from UNIX user: {}", unix_user.username);

        match session_handler(conn, &unix_user, &config).await {
            Ok(()) => {}
            Err(e) => {
                log::error!("Failed to run server: {}", e);
            }
        }
    }

    Ok(())
}
