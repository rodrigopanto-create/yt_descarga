use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

const MAX_OUTPUT_BYTES: u64 = 16 * 1024;

pub(crate) struct ProcessOutput {
    pub estado: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

pub(crate) fn ejecutar_con_timeout(
    mut command: Command,
    timeout: Duration,
) -> Result<ProcessOutput, String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("No se pudo iniciar el proceso: {error}"))?;

    let stdout = read_bounded(child.stdout.take());
    let stderr = read_bounded(child.stderr.take());
    let estado = wait_with_timeout(&mut child, timeout)?;
    let output_timeout = Duration::from_secs(2);

    Ok(ProcessOutput {
        estado,
        stdout: stdout
            .recv_timeout(output_timeout)
            .map_err(|error| format!("No se pudo leer stdout del proceso: {error}"))??,
        stderr: stderr
            .recv_timeout(output_timeout)
            .map_err(|error| format!("No se pudo leer stderr del proceso: {error}"))??,
    })
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Result<ExitStatus, String> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                child
                    .kill()
                    .map_err(|error| format!("No se pudo terminar el proceso vencido: {error}"))?;
                let _ = child.wait();
                return Err(format!(
                    "El proceso excedió el tiempo límite de {} s y fue terminado",
                    timeout.as_secs_f32()
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("No se pudo esperar al proceso: {error}"));
            }
        }
    }
}

fn read_bounded<R: Read + Send + 'static>(pipe: Option<R>) -> Receiver<Result<String, String>> {
    let (sender, receiver) = mpsc::channel();
    if let Some(mut pipe) = pipe {
        thread::spawn(move || {
            let mut buffer = Vec::new();
            let result = (&mut pipe)
                .take(MAX_OUTPUT_BYTES)
                .read_to_end(&mut buffer)
                .map(|_| String::from_utf8_lossy(&buffer).into_owned())
                .map_err(|error| format!("Error leyendo salida del proceso: {error}"));
            if let Err(error) = io::copy(&mut pipe, &mut io::sink()) {
                let _ = sender.send(Err(format!(
                    "Error drenando la salida restante del proceso: {error}"
                )));
            } else {
                let _ = sender.send(result);
            }
        });
    }
    receiver
}
