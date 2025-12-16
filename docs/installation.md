# Installation and configuration

This document contains instructions for the recommended way of installing and configuring muscl.

Note that there are separate instructions for [installing on NixOS](nixos.md) and [installing with SUID/SGID mode](suid-sgid-mode.md).

## Installing with deb on Debian

You can install muscl by adding the [PVV apt repository][pvv-apt-repository] and installing the package:

```bash
# Become root (if not already)
sudo -i

# Check the version of your Debian installation
VERSION_CODENAME=$(lsb_release -cs)

# Add the repository
echo "deb [arch=amd64 signed-by=/etc/apt/keyrings/pvvgit-projects.asc] https://git.pvv.ntnu.no/api/packages/Projects/debian $VERSION_CODENAME main" | tee -a /etc/apt/sources.list.d/gitea.list

# Pull the repository key
curl https://git.pvv.ntnu.no/api/packages/Projects/debian/repository.key -o /etc/apt/keyrings/pvvgit-projects.asc

# Update package lists
apt update

# Install muscl
apt install muscl
```

## Creating a database user

In order for the daemon to be able to do anything interesting on the mysql server, it needs
a database user with sufficient privileges. You can create such a user by running the following commands
on the mysql server as the admin user (or another user with sufficient privileges):

```sql
CREATE USER `muscl`@`%` IDENTIFIED BY '<strong_password_here>';
GRANT SELECT, INSERT, UPDATE, DELETE ON `mysql`.* TO `muscl`@`%`;
GRANT GRANT OPTION, CREATE, DROP ON *.* TO 'muscl'@'%';
FLUSH PRIVILEGES;
```

Make sure to remember the username and password, as we will now need to add them to the muscl configuration.

The configuration already comes preconfigured expecting the database user to be named `muscl`.
If you named it differently, please edit `/etc/muscl/muscl.conf` accordingly.

For systemd-based setups, we recommend using `systemd-creds` to provide the database password, see the section below.

## Setting the myscl password ...

### ... with `systemd-creds`

The debian package assumes that you will provide the password for `muscl`'s database user with `systemd-creds`.

You can add the password like this:

```bash
# Become root (if not already)
sudo -i

# Unless you already have a working credential store, you need to set it up first
mkdir -p /etc/credstore.encrypted
systemd-creds setup

# Be careful not to leave the password in your shell history!
# Add a space before setting the next line to avoid this.
 export MUSCL_MYSQL_PASSWORD="<strong_password_here>"

# Now set the muscl mysql password
systemd-creds encrypt --name=muscl_mysql_password <(echo "$MUSCL_MYSQL_PASSWORD") /etc/credstore.encrypted/muscl_mysql_password
```

If you are running systemd older than version 254 (see `systemctl --version`), you might have to override the service to point to the path of the credential manually, because `ImportCredential=` is not supported. Run `systemctl edit muscl.service` and add the following lines:

```ini
[Service]
LoadCredentialEncrypted=muscl_mysql_password:/etc/credstore.encrypted/muscl_mysql_password
```

### ... without `systemd-creds`

If you do not have systemd, or if you do not want to use `systemd-creds`, you can also set the password in any other file on the system.
Be careful to ensure that the file is not readable by unprivileged users, as it would yield them too much access to the mysql server.
Edit `/etc/muscl/muscl.conf` and set the `mysql_password_file` option below `[database]` to point to the file containing the password.

If you are using systemd, you should also create an override to unset the `ImportCredential=` line. Run `systemctl edit muscl.service` and add the following lines:

```ini
[Service]
ImportCredential=
```

## Configuring group denylists

In `/etc/muscl/muscl.conf`, you will find an option below `[authorization]` named `group_denylist_file`,
which points to `/etc/muscl/group_denylist.txt` by default.

In this file, you can add unix group names or GIDs to disallow the groups from being used as prefixes.

The deb package comes with a default denylist that disallows some common system groups.

The format of the file is one group name or GID per line. Lines starting with `#` and empty lines are ignored.

```
# Disallow using the 'root' group as a prefix
gid:0

# Disallow using the 'adm' group as a prefix
group:adm
```

> [!NOTE]
> If a user is named the same as a disallowed group, that user will still be able to use their username as a prefix.

## A note on minimum version requirements

The muscl server will work with older versions of systemd, but the recommended version is 254 or newer.

For full landlock support (disabled by default), you need a linux kernel version 6.7 or newer.

[pvv-apt-repository]: https://git.pvv.ntnu.no/Projects/-/packages/debian/muscl
