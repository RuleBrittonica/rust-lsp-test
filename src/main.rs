#![allow(clippy::print_stderr)]

use std::error::Error;
use std::fs::File;
use std::io::{
    self,
    BufRead,
    BufReader,
    Write
};
use std::sync::{
    Arc,
    Mutex
};
use std::thread;
use std::time::Duration;
use lsp_types::OneOf;
use lsp_types::{
    request::GotoDefinition,
    GotoDefinitionResponse,
    InitializeParams,
    ServerCapabilities,
    CodeActionProviderCapability,
};
use lsp_server::{
    Connection, ExtractError, Message, ReqQueue, Request, RequestId, Response
};

use serde::{
    Deserialize,
    Serialize
};
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Command {
    jsonrpc: String,
    method: String,
    id: Option<i32>,
    params: Value,
}

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    eprintln!("Starting LSP Server");

    let (connection, io_threads) = Connection::stdio();
    let connection = Arc::new(Mutex::new(connection)); // Wrap connection in Arc<Mutex>

    eprintln!("Connection established");

    let server_capabilities = serde_json::to_value(&ServerCapabilities {
        definition_provider: Some(OneOf::Left(true)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        ..Default::default()
    })?;

    eprintln!("Initializing LSP Server");

    // Send initial commands
    // send_initial_commands(&connection)?;

    let initialization_params = match connection.lock().unwrap().initialize(server_capabilities) {
        Ok(it) => it,
        Err(e) => {
            if e.channel_is_disconnected() {
                io_threads.join()?;
            }
            return Err(e.into());
        }
    };

    eprintln!("Initialized with params: {:#?}", initialization_params);

    // These txt files contain the exact command that would normally be pasted
    // into std in. We will read these files and send the commands to the LSP
    let command_file_paths = vec![
        "src/json/goto.json",
        "src/json/shutdown.json",
        "src/json/exit.json",
    ];

    // Start the input handler in a separate thread
    // let connection_clone = Arc::clone(&connection);
    // // TODO: Read in the input files once every second, and send the commands to
    // // the server
    // thread::spawn(move || {
    //     loop {
    //         // Wait for a second before checking for commands
    //         thread::sleep(Duration::from_secs(1));
    //         for file_path in &command_file_paths {
    //             if let Err(e) = read_and_send_command(&connection_clone, file_path) {
    //                 eprintln!("Error reading from {}: {:?}", file_path, e);
    //             }
    //         }
    //     }
    // });

    main_loop(Arc::clone(&connection), initialization_params)?;
    io_threads.join()?;

    eprintln!("Shutting down LSP Server");
    Ok(())
}

fn send_initial_commands(connection: &Arc<Mutex<Connection>>) -> Result<(), Box<dyn Error + Sync + Send>> {
    // Specify the commands you want to send initially
    let initial_commands = vec![
        "src/json/initialize.json",
        "src/json/initialized.json",
    ];

    for file_path in initial_commands {
        if let Err(e) = read_and_send_command(connection, file_path) {
            eprintln!("Error sending initial command from {}: {:?}", file_path, e);
        }
    }

    Ok(())
}

fn read_and_send_command(connection: &Arc<Mutex<Connection>>, file_path: &str) -> Result<(), Box<dyn Error + Sync + Send>> {
    // Open the command file
    let file = File::open(file_path)?;
    let reader: BufReader<File> = BufReader::new(file);

    // Read the json in from the file
    let file_contents: String = reader
        .lines()
        .filter_map(|line| line.ok()) // Filter out errors
        .collect::<Vec<_>>() // Collect lines into a Vec
        .join("\n"); // Join with newline to ensure valid JSON if needed

    eprintln!("Reading command from {}: {}", file_path, file_contents);

    // Ensure the JSON is valid
    let command: Command = serde_json::from_str(&file_contents)?;

    // Send the command to the LSP server
    let conn = connection.lock().unwrap();
    conn.sender.send(Message::Request(Request {
        id: RequestId::from(command.id.unwrap_or(1)), // Handle IDs appropriately
        method: command.method,
        params: command.params,
    }))?;

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
