use std::fmt::Display;

use cosmic_settings_printers_client::{self as printers_client};

pub async fn open_printer_web_page(web_page: String) -> Result<(), String> {
    let status = tokio::process::Command::new("xdg-open")
        .arg(&web_page)
        .status()
        .await
        .map_err(|why| format!("failed to run xdg-open for {web_page}: {why}"))?;

    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("xdg-open exited with {status} for {web_page}"))
}

pub async fn set_printer_default(printer_id: String) -> Result<(), String> {
    let mut client = printers_client::connect().await.map_err(display_error)?;

    client
        .set_printer_default(&printer_id)
        .await
        .map_err(display_error)
}

fn display_error(error: impl Display) -> String {
    error.to_string()
}

pub async fn delete_printer(printer_id: String) -> Result<(), String> {
    let mut client = printers_client::connect().await.map_err(display_error)?;

    client
        .delete_printer(&printer_id)
        .await
        .map_err(display_error)
}

pub async fn set_printer_location(printer_id: String, location: String) -> Result<(), String> {
    let mut client = printers_client::connect().await.map_err(display_error)?;

    client
        .set_printer_location(&printer_id, &location)
        .await
        .map_err(display_error)
}

pub async fn set_printer_option_default(
    printer_id: String,
    option: String,
    value: String,
) -> Result<(), String> {
    let mut client = printers_client::connect().await.map_err(display_error)?;

    client
        .set_printer_option_default(&printer_id, &option, &[value])
        .await
        .map_err(display_error)
}
