use clap::Parser;

#[derive(Parser)]
pub struct DatabaseArgs {
    #[clap(subcommand)]
    subcmd: DatabaseCommand,
}

#[derive(Parser)]
enum DatabaseCommand {
    /// Create the DATABASE(S).
    Create,

    /// Delete the DATABASE(S).
    Drop,

    /// Give information about the DATABASE(S), or, if none are given, all the ones you own.
    Show,

    /// Change permissions for the DATABASE(S).
    /// Your favorite editor will be started, allowing you to make changes to the permission table.
    /// Run `mysql-dbadm --help-editperm` for more information.
    EditPerm,
}

pub fn handle_command(args: DatabaseArgs) {
    match args.subcmd {
        DatabaseCommand::Create => println!("Creating database"),
        DatabaseCommand::Drop => println!("Dropping database"),
        DatabaseCommand::Show => println!("Showing database"),
        DatabaseCommand::EditPerm => println!("Editing permissions"),
    }
}