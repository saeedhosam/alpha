use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::Duration;

use chromiumoxide::Browser;
use futures::StreamExt;
use serde::Deserialize;

use alpha::{
    TicketCsvRow, extract_airline, extract_airline_code, extract_basic, extract_basic_from_twd,
    extract_doc_type, extract_fop, extract_issue_date, extract_name_from_detail, extract_route,
    extract_sign, extract_ticket_key, extract_ticket_no, extract_total, extract_tour_code,
    format_csv_row, has_more_pages, parse_tjq_data_lines, route_to_cities,
};

#[derive(Debug, Deserialize)]
struct ChromeTarget {
    id: String,
    url: String,
    #[serde(rename = "type")]
    target_type: String,
}

async fn send_command(
    page: &chromiumoxide::Page,
    cmd: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let selector = "#cryptics1_cmd_shellbridge_shellWindow_top_left_modeString_cmdPromptInput";
    let input = page.find_element(selector).await?;
    input.click().await?;
    input.type_str(cmd).await?;
    input.press_key("Enter").await?;
    wait_for_response(page).await
}

async fn wait_for_response(
    page: &chromiumoxide::Page,
) -> Result<String, Box<dyn std::error::Error>> {
    let selector = "#cryptics1_cmd_shellbridge_shellWindow_top_left_modeString_currentCommand .command .cmdResponse";
    let timeout = Duration::from_secs(30);
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            return Err("Timed out waiting for Amadeus response".into());
        }
        if let Ok(el) = page.find_element(selector).await {
            if let Ok(Some(text)) = el.inner_text().await {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Ok(trimmed.to_string());
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: alpha <date>");
        eprintln!("Example: alpha 19jul");
        std::process::exit(1);
    }

    let date = args[1].to_uppercase();

    let targets: Vec<ChromeTarget> = reqwest::get("http://127.0.0.1:9222/json/list")
        .await?
        .json()
        .await?;

    let amadeus_target = targets
        .into_iter()
        .find(|t| t.target_type == "page" && t.url.contains("sellingplatformconnect.amadeus.com"))
        .ok_or("Amadeus tab not found!")?;

    println!("Found Amadeus Tab ID: {}", amadeus_target.id);

    let (mut browser, mut handler) = Browser::connect("http://127.0.0.1:9222").await?;
    tokio::spawn(async move { while handler.next().await.is_some() {} });

    browser.fetch_targets().await?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let pages = browser.pages().await?;
    let target_page = pages
        .into_iter()
        .find(|p| p.target_id().as_ref() == amadeus_target.id)
        .ok_or("Could not attach to matching page in chromiumoxide")?;

    let mut seen = HashSet::new();
    let mut all_tickets: Vec<String> = Vec::new();

    println!("Executing: TJQ/SOF/D-{}", date);
    let mut response = send_command(&target_page, &format!("TJQ/SOF/D-{}", date)).await?;
    loop {
        println!("TJQ response received, parsing...");
        for line in parse_tjq_data_lines(&response) {
            if let Ok(key) = extract_ticket_key(&line) {
                if seen.insert(key) {
                    all_tickets.push(line);
                }
            }
        }

        if !has_more_pages(&response) {
            break;
        }
        println!("More documents available, sending MD...");
        response = send_command(&target_page, "MD").await?;
    }

    if all_tickets.is_empty() {
        eprintln!("No tickets found for date {}", date);
        std::process::exit(1);
    }

    let mut csv_lines = Vec::new();
    for (i, tjq_line) in all_tickets.iter().enumerate() {
        let airline_code = extract_airline_code(tjq_line)?;
        let ticket_no = extract_ticket_no(tjq_line)?;
        let doc_type = extract_doc_type(tjq_line)?;

        let detail_cmd = if doc_type == "EMD" {
            format!("EWD/EMD{}-{}", airline_code, ticket_no)
        } else {
            format!("TWD/TKT{}-{}", airline_code, ticket_no)
        };

        println!(
            "  [{}/{}] Fetching: {}",
            i + 1,
            all_tickets.len(),
            detail_cmd
        );
        let detail_response = send_command(&target_page, &detail_cmd).await?;

        let is_emd = doc_type == "EMD";
        let is_void = doc_type == "Void";

        let row = TicketCsvRow {
            airline: extract_airline(tjq_line)?,
            ticket_no,
            doc_type,
            name: extract_name_from_detail(&detail_response)?,
            issue_date: extract_issue_date(&date),
            route: if is_emd {
                String::new()
            } else {
                route_to_cities(&extract_route(&detail_response).unwrap_or_default())
            },
            tour_code: if is_emd {
                String::new()
            } else {
                extract_tour_code(&detail_response).unwrap_or_default()
            },
            basic: if is_void {
                "0.00".to_string()
            } else if is_emd {
                extract_total(tjq_line)?
            } else {
                extract_basic_from_twd(&detail_response)
                    .unwrap_or_else(|_| extract_basic(tjq_line).unwrap_or_default())
            },
            total: if is_void {
                "0.00".to_string()
            } else {
                extract_total(tjq_line)?
            },
            fop: extract_fop(tjq_line)?,
            sign: extract_sign(tjq_line)?,
        };
        csv_lines.push(format_csv_row(&row));
    }

    let dir = Path::new("tickets");
    fs::create_dir_all(dir)?;
    let filepath = dir.join(format!("{}.csv", date));
    fs::write(&filepath, csv_lines.join("\n"))?;

    println!(
        "Written {} ticket(s) to {}",
        csv_lines.len(),
        filepath.display()
    );
    Ok(())
}
