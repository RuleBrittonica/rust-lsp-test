#![allow(clippy::print_stderr)]

use std::error::Error;
use std::io::{
    self,
    BufRead,
    Write
};
use std::sync::{
    Arc,
    Mutex
};
use std::thread;
use lsp_types::OneOf;
use lsp_types::{
    request::GotoDefinition,
    GotoDefinitionResponse,
    InitializeParams,
    ServerCapabilities,
    CodeActionProviderCapability,
};
use lsp_server::{
    Connection,
    ExtractError,
    Message,
    Request,
    RequestId,
    Response
};

use serde::{
    Deserialize,
    Serialize
};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Command {
    jsonrpc: String,
    method: String,
    id: Option<u64>,
    params: Option<serde_json::Value>,
}

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    eprintln!("Starting LSP Server");

    let (connection, io_threads) = Connection::stdio();
    let connection = Arc::new(Mutex::new(connection)); // Wrap connection in Arc<Mutex>

    let server_capabilities = serde_json::to_value(&ServerCapabilities {
        definition_provider: Some(OneOf::Left(true)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        ..Default::default()
    })?;

    let initialization_params = match connection.lock().unwrap().initialize(server_capabilities) {
        Ok(it) => it,
        Err(e) => {
            if e.channel_is_disconnected() {
                io_threads.join()?;
            }
            return Err(e.into());
        }
    };

    // Start the input handler in a separate thread
    let connection_clone = Arc::clone(&connection);
    thread::spawn(move || {
        let stdin = io::stdin();
        let reader = stdin.lock();
        for line in reader.lines() {
            if let Ok(command_input) = line {
                // Here, command_input should be an index referring to the commands loaded from JSON
                if let Ok(command_index) = command_input.parse::<usize>() {
                    if let Err(e) = send_command(&connection_clone, command_index) {
                        eprintln!("Error sending command: {e}");
                    }
                } else {
                    eprintln!("Invalid command input: {command_input}");
                }
            }
        }
    });

    main_loop(Arc::clone(&connection), initialization_params)?;
    io_threads.join()?;

    eprintln!("Shutting down LSP Server");
    Ok(())
}

fn load_commands(filepaths: &[&str]) -> Result<Vec<Command>, Box<dyn Error>> {
    let mut commands: Vec<Command> = Vec::new();
    for &filepath in filepaths {
        let file = std::fs::File::open(filepath)?;
        let reader = io::BufReader::new(file);
        let mut loaded_commands: Vec<Command> = serde_json::from_reader(reader)?;
        commands.append(&mut loaded_commands); // Add loaded commands to the main vector
    }
    Ok(commands)
}

fn send_command(connection: &Arc<Mutex<Connection>>, command_index: usize) -> Result<(), Box<dyn Error + Sync + Send>> {
    let conn = connection.lock().unwrap(); // Lock the connection

    let filepaths = vec![
        "src/json/initialize.json",
        "src/json/initialized.json",
        "src/json/goto.json",
        "src/json/shutdown.json",
        "src/json/exit.json",
    ];

    // Load all commands from the specified JSON files
    let commands: Vec<Command> = load_commands(&filepaths).unwrap();

    // Ensure the command index is valid
    if command_index >= commands.len() {
        return Err("Invalid command index".into());
    }

    // Get the command to send
    let command: Command = commands[command_index].clone();

    // Create a Message::Request from the loaded command
    let request_message: Message = Message::Request(Request {
        id: RequestId::from(1), // Fallback to a default ID
        method: command.method,
        params: command.params.unwrap(),
    });

    // Send the request over the connection
    conn.sender.send(request_message)?; // Send the request message

    Ok(())
}

fn main_loop(
    connection: Arc<Mutex<Connection>>,
    params: serde_json::Value,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    let _params: InitializeParams = serde_json::from_value(params)?;
    eprintln!("Starting Main Loop");
    loop {
        let msg = {
            let conn = connection.lock().unwrap();
            match conn.receiver.recv() {
                Ok(msg) => msg,
                Err(_) => break, // Exit on error
            }
        };

        match msg {
            Message::Request(req) => {
                if connection.lock().unwrap().handle_shutdown(&req)? {
                    return Ok(());
                }
                eprintln!("got request: {req:?}");
                match cast::<GotoDefinition>(req) {
                    Ok((id, params)) => {
                        eprintln!("got gotoDefinition request #{id}: {params:?}");
                        let result = Some(GotoDefinitionResponse::Array(Vec::new()));
                        let result = serde_json::to_value(&result)?;
                        let resp = Response { id, result: Some(result), error: None };
                        connection.lock().unwrap().sender.send(Message::Response(resp))?;
                        continue;
                    }
                    Err(err @ ExtractError::JsonError { .. }) => panic!("{err:?}"),
                    Err(ExtractError::MethodMismatch(req)) => req,
                };
            }
            Message::Response(resp) => {
                eprintln!("got response: {resp:?}");
            }
            Message::Notification(not) => {
                eprintln!("got notification: {not:?}");
            }
        }
    }
    Ok(())
}

fn cast<R>(req: Request) -> Result<(RequestId, R::Params), ExtractError<Request>>
where
    R: lsp_types::request::Request,
    R::Params: serde::de::DeserializeOwned,
{
    req.extract(R::METHOD)
}
