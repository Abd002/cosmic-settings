use std::fmt::{Debug, Display};

use cosmic_settings_printers_client::{self as printers_client, CosmicPrintersProxy};

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
        .conn
        .set_printer_default(printer_id)
        .await
        .map_err(display_error)?
        .map_err(debug_error)
}

fn display_error(error: impl Display) -> String {
    error.to_string()
}

fn debug_error(error: impl Debug) -> String {
    format!("{error:?}")
}

pub async fn delete_printer(printer_id: String) -> Result<(), String> {
    let mut client = printers_client::connect().await.map_err(display_error)?;

    client
        .conn
        .delete_printer(printer_id)
        .await
        .map_err(display_error)?
        .map_err(debug_error)
}

pub async fn set_printer_location(printer_id: String, location: String) -> Result<(), String> {
    let mut client = printers_client::connect().await.map_err(display_error)?;

    client
        .conn
        .set_printer_location(printer_id, location)
        .await
        .map_err(display_error)?
        .map_err(debug_error)
}

pub async fn set_printer_option_default(
    printer_id: String,
    option: String,
    value: String,
) -> Result<(), String> {
    let mut client = printers_client::connect().await.map_err(display_error)?;

    client
        .conn
        .set_printer_option_default(printer_id, option, vec![value])
        .await
        .map_err(display_error)?
        .map_err(debug_error)
}
