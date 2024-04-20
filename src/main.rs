use clap::Parser;

mod database_command;
mod user_command;

#[derive(Parser)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Parser)]
enum Command {
    /// Create, drop or edit permission for the DATABASE(s),
    #[clap(name = "db")]
    Database(database_command::DatabaseArgs),

    /// Create, delete or change password for your USER,
    #[clap(name = "user")]
    User(user_command::UserArgs),
}

fn main() {
    let args: Args = Args::parse();
    match args.command {
        Command::Database(database_args) => database_command::handle_command(database_args),
        Command::User(user_args) => user_command::handle_command(user_args),
    }
}

// loginstud03% mysql-dbadm --help

// Usage: mysql-dbadm COMMAND [DATABASE]...
// Create, drop og edit permission for the DATABASE(s),
// as determined by the COMMAND.  Valid COMMANDs:

//   create     create the DATABASE(s).
//   drop       delete the DATABASE(s).
//   show       give information about the DATABASE(s), or, if
//              none are given, all the ones you own.
//   editperm   change permissions for the DATABASE(s).  Your
//              favorite editor will be started, allowing you
//              to make changes to the permission table.
//              Run 'mysql-dbadm --help-editperm' for more
//              information.

// Report bugs to orakel@ntnu.no

// loginstud03% mysql-useradm --help

// Usage: mysql-useradm COMMAND [USER]...
// Create, delete or change password for the USER(s),
// as determined by the COMMAND.  Valid COMMANDs:

//   create     create the USER(s).
//   delete     delete the USER(s).
//   passwd     change the MySQL password for the USER(s).
//   show       give information about the USERS(s), or, if
//              none are given, all the users you have.

// Report bugs to orakel@ntnu.no
