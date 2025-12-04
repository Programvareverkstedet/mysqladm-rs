# Installation and configuration

## Installing with deb on Debian

You can install muscl by adding the muscl apt repository and installing the package:

```bash
# Check the version of your Debian installation
VERSION_CODENAME=$(lsb_release -cs)

# Add the repository
echo "deb [signed-by=/etc/apt/keyrings/pvvgit-projects.asc] https://git.pvv.ntnu.no/api/packages/Projects/debian $VERSION_CODENAME main" | sudo tee -a /etc/apt/sources.list.d/gitea.list

# Pull the repository key
sudo curl https://git.pvv.ntnu.no/api/packages/Projects/debian/repository.key -o /etc/apt/keyrings/pvvgit-projects.asc

# Update package lists
sudo apt update

# Install muscl
sudo apt install muscl
```

## Creating a database user

In order for the daemon to be able to do anything interesting on the mysql server, it needs
a database user with sufficient privileges. You can create such a user by running the following commands
on the mysql server as root (or another user with sufficient privileges):

```sql
CREATE USER `muscl`@`%` IDENTIFIED BY '<strong_password_here>';
GRANT SELECT, INSERT, UPDATE, DELETE ON `mysql`.* TO `muscl`@`%`;
GRANT GRANT OPTION, CREATE, DROP ON *.* TO 'muscl'@'%';
FLUSH PRIVILEGES;
```

Now you should add the login credentials to the muscl configuration file, typically located at `/etc/muscl/config.toml`.

## Setting the myscl password with `systemd-creds`

The debian package assumes that you will provide the password for `muscl`'s database user with `systemd-creds`.

You can add the password like this (run as root):

```bash
# Unless you already have a working credential store, you need to set it up first
mkdir -p /etc/credstore.encrypted
systemd-creds setup

# Now set the muscl mysql password
# Be careful not to leave the password in your shell history!
systemd-creds encrypt --name=muscl_mysql_password <(echo "<strong_password_here>") /etc/credstore.encrypted/muscl_mysql_password
```

If you are running systemd older than version 254 (see `systemctl --version`), you might have to override the service to point to the path of the credential manually, because `ImportCredential=` is not supported. Run `systemctl edit muscl.service` and add the following lines:

```ini
[Service]
LoadCredentialEncrypted=muscl_mysql_password:/etc/credstore.encrypted/muscl_mysql_password
```

## SUID/SGID mode

For backwards compatibility reasons, it is possible to run the program without a daemon by utilizing SUID/SGID.
In order to do this, you should set either the SUID/SGID bit and preferably make the executable owned by a non-privileged user.
If the database is running on the same machine, the user/group will need access to write and read from the database socket.
Otherwise, the only requirement is that the user/group is able to read the config file (typically `/etc/muscl/config.toml`).
