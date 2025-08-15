use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "task")]
pub struct Task {
    #[command(subcommand)]
    pub subcommand: TaskSubcommand,
}

#[derive(Debug, Parser)]
pub enum TaskSubcommand {
    Create(CreateTaskCommand),
    List(ListTasksCommand),
}

#[derive(Debug, Parser)]
pub struct CreateTaskCommand {
    #[arg(short, long)]
    pub name: String,
}

#[derive(Debug, Parser)]
pub struct ListTasksCommand {}
