use clap::Parser;

#[derive(Parser)]
pub struct UserArgs {
    #[clap(subcommand)]
    subcmd: UserCommand,
}

#[derive(Parser)]
enum UserCommand {
    Create,
    Delete,
    Passwd,
    Show,
}

pub fn handle_command(args: UserArgs) {
    match args.subcmd {
        UserCommand::Create => println!("Creating user"),
        UserCommand::Delete => println!("Deleting user"),
        UserCommand::Passwd => println!("Changing password"),
        UserCommand::Show => println!("Showing user"),
    }
}