use std::collections::HashMap;
use std::fmt;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq)]
pub enum ExtractError {
    ParseError(String),
    NotFound(String),
    InvalidFormat(String),
}

impl fmt::Display for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExtractError::ParseError(msg) => write!(f, "parse error: {}", msg),
            ExtractError::NotFound(msg) => write!(f, "not found: {}", msg),
            ExtractError::InvalidFormat(msg) => write!(f, "invalid format: {}", msg),
        }
    }
}

impl std::error::Error for ExtractError {}

fn airline_3to2() -> [(&'static str, &'static str); 25] {
    [
        ("876", "3U"), ("390", "A3"), ("057", "AF"), ("147", "AT"),
        ("055", "AZ"), ("125", "BA"), ("176", "EK"), ("075", "IB"),
        ("220", "LH"), ("080", "LO"), ("724", "LX"), ("076", "ME"),
        ("077", "MS"), ("781", "MU"), ("477", "NE"), ("325", "NP"),
        ("257", "OS"), ("157", "QR"), ("512", "RJ"), ("381", "SM"),
        ("065", "SV"), ("235", "TK"), ("199", "TU"), ("204", "VF"),
        ("910", "WY"),
    ]
}

pub fn map_airline(code_3digit: &str) -> Result<&'static str, ExtractError> {
    airline_3to2()
        .iter()
        .find(|(code, _)| *code == code_3digit)
        .map(|(_, name)| *name)
        .ok_or_else(|| ExtractError::NotFound(format!("unknown airline code: {}", code_3digit)))
}

fn parse_csv_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let mut chars = line.chars().peekable();
    let mut iata = String::new();
    let mut city = String::new();
    let mut in_quotes = false;
    let mut field = 0;
    let mut current = String::new();

    while let Some(c) = chars.next() {
        if field == 0 && c == ',' {
            iata = current.trim().to_string();
            current.clear();
            field = 1;
            continue;
        }
        if field == 1 {
            if c == '"' {
                in_quotes = !in_quotes;
                continue;
            }
            if c == ',' && !in_quotes {
                break;
            }
        }
        current.push(c);
    }
    if field == 1 {
        city = current.trim().to_string();
    }

    if iata.is_empty() {
        return None;
    }
    Some((iata, city))
}

static AIRPORT_CITIES: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    let csv = include_str!("../data/airport_city_rows.csv");
    let mut map = HashMap::new();
    for line in csv.lines().skip(1) {
        if let Some((iata, city)) = parse_csv_line(line) {
            map.insert(iata, city);
        }
    }
    map
});

pub fn airport_to_city(code: &str) -> Option<String> {
    AIRPORT_CITIES.get(code).cloned()
}

pub fn route_to_cities(route: &str) -> String {
    route
        .split('-')
        .map(|code| airport_to_city(code).unwrap_or_else(|| code.to_string()))
        .collect::<Vec<_>>()
        .join("-")
}

pub fn map_doc_type(trnc: &str, fop: &str) -> String {
    match trnc {
        "TKTT" if fop == "XA" => "Reissue".into(),
        "TKTT" => "Ticket".into(),
        "RFND" => "Refund".into(),
        "CANX" | "CANN" => "Void".into(),
        "EMDA" | "EMDS" => "EMD".into(),
        other => other.into(),
    }
}

pub fn map_fop(fop: &str) -> String {
    match fop {
        "XA" | "CA" => "Cash".into(),
        other => other.into(),
    }
}

fn sign_map() -> [(&'static str, &'static str); 4] {
    [
        ("AE", "Amal"),
        ("AS", "Amira"),
        ("MA", "Noha"),
        ("SH", "Saeed"),
    ]
}

pub fn map_sign(code: &str) -> Result<&'static str, ExtractError> {
    sign_map()
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, name)| *name)
        .ok_or_else(|| ExtractError::NotFound(format!("unknown sign code: {}", code)))
}

fn tokenize_tjq_line(line: &str) -> Result<Vec<&str>, ExtractError> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 10 {
        return Err(ExtractError::InvalidFormat(
            "TJQ line has fewer than 10 fields".into(),
        ));
    }
    Ok(tokens)
}

fn is_compact_void(tokens: &[&str]) -> bool {
    tokens.len() == 10 && matches!(tokens.last(), Some(&"CANX") | Some(&"CANN"))
}

fn parse_amount(s: &str) -> f64 {
    let clean: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    clean.parse::<f64>().unwrap_or(0.0)
}

pub fn extract_airline_code(line: &str) -> Result<String, ExtractError> {
    let tokens = tokenize_tjq_line(line)?;
    tokens[0]
        .split('*')
        .nth(1)
        .map(|s| s.to_string())
        .ok_or_else(|| ExtractError::ParseError("SEQ NO field missing '*' separator".into()))
}

pub fn extract_airline(line: &str) -> Result<String, ExtractError> {
    let code = extract_airline_code(line)?;
    map_airline(&code).map(|s| s.to_string())
}

pub fn extract_ticket_no(line: &str) -> Result<String, ExtractError> {
    let tokens = tokenize_tjq_line(line)?;
    Ok(tokens[1].to_string())
}

pub fn extract_doc_type(line: &str) -> Result<String, ExtractError> {
    let tokens = tokenize_tjq_line(line)?;
    let trnc = tokens.last().unwrap();
    let fop = if is_compact_void(&tokens) {
        ""
    } else {
        tokens[6]
    };
    Ok(map_doc_type(trnc, fop))
}

pub fn extract_name(line: &str) -> Result<String, ExtractError> {
    let tokens = tokenize_tjq_line(line)?;
    tokens
        .iter()
        .find(|t| t.contains('/'))
        .map(|t| t.to_string())
        .ok_or_else(|| {
            ExtractError::NotFound("passenger name not found (no '/' in any field)".into())
        })
}

pub fn extract_name_from_detail(response: &str) -> Result<String, ExtractError> {
    for line in response.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("1.") {
            if let Some(name) = rest.split("  ").next() {
                let name = name.trim();
                if !name.is_empty() {
                    return Ok(name.to_string());
                }
            }
        }
        if let Some(rest) = trimmed.strip_prefix("PAX- ") {
            if let Some(name) = rest.split("  ").next() {
                let name = name.trim();
                if !name.is_empty() {
                    return Ok(name.to_string());
                }
            }
        }
    }
    Err(ExtractError::NotFound(
        "name not found in TWD/EWD response".into(),
    ))
}

pub fn extract_issue_date(date: &str) -> String {
    date.to_uppercase()
}

pub fn extract_route(response: &str) -> Result<String, ExtractError> {
    let mut cities: Vec<String> = Vec::new();
    let mut in_itinerary = false;

    for line in response.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        if tokens[0].chars().all(|c| c.is_ascii_digit())
            && tokens.len() >= 2
            && (tokens[1].starts_with('O') || tokens[1].starts_with('X'))
            && tokens[1].len() >= 4
        {
            let city = &tokens[1][1..4];
            if city.chars().all(|c| c.is_ascii_uppercase()) {
                cities.push(city.to_string());
                in_itinerary = true;
                continue;
            }
        }

        if in_itinerary
            && tokens.len() == 1
            && tokens[0].len() == 3
            && tokens[0].chars().all(|c| c.is_ascii_uppercase())
        {
            cities.push(tokens[0].to_string());
            break;
        }
    }

    if cities.len() < 2 {
        return Err(ExtractError::NotFound(
            "could not extract route from TWD response".into(),
        ));
    }

    Ok(cities.join("-"))
}

pub fn extract_tour_code(response: &str) -> Result<String, ExtractError> {
    for line in response.lines() {
        if let Some(rest) = line.trim().strip_prefix("FT ") {
            let tour = rest.trim();
            if !tour.is_empty() {
                return Ok(tour.to_string());
            }
        }
    }
    Err(ExtractError::NotFound(
        "tour code (FT) not found in TWD response".into(),
    ))
}

pub fn extract_basic(line: &str) -> Result<String, ExtractError> {
    let tokens = tokenize_tjq_line(line)?;
    let total = parse_amount(tokens[2]);
    let tax = parse_amount(tokens[3]);
    Ok(format!("{:.2}", total - tax))
}

pub fn extract_basic_from_twd(response: &str) -> Result<String, ExtractError> {
    for line in response.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("EQUIV") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 3 && parts[1] != "BSR" && parts[2].chars().any(|c| c.is_ascii_digit()) {
                return Ok(parts[2].to_string());
            }
        }
    }
    Err(ExtractError::NotFound(
        "EQUIV amount not found in TWD response".into(),
    ))
}

pub fn extract_total(line: &str) -> Result<String, ExtractError> {
    let tokens = tokenize_tjq_line(line)?;
    Ok(format!("{:.2}", parse_amount(tokens[2])))
}

pub fn extract_fop(line: &str) -> Result<String, ExtractError> {
    let tokens = tokenize_tjq_line(line)?;
    Ok(map_fop(if is_compact_void(&tokens) {
        ""
    } else {
        tokens[6]
    }))
}

pub fn extract_sign(line: &str) -> Result<String, ExtractError> {
    let tokens = tokenize_tjq_line(line)?;
    let code = if is_compact_void(&tokens) {
        tokens[7]
    } else {
        tokens[8]
    };
    map_sign(code).map(|s| s.to_string())
}

pub fn parse_tjq_data_lines(response: &str) -> Vec<String> {
    let mut lines = Vec::new();

    for line in response.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("---") {
            continue;
        }
        if trimmed.contains("SEQ NO") {
            continue;
        }
        let first_token = trimmed.split_whitespace().next().unwrap_or("");
        if !first_token.contains('*') {
            continue;
        }
        lines.push(line.to_string());
    }

    lines
}

pub fn extract_ticket_key(line: &str) -> Result<String, ExtractError> {
    let tokens = tokenize_tjq_line(line)?;
    let code = tokens[0]
        .split('*')
        .nth(1)
        .ok_or_else(|| ExtractError::ParseError("SEQ NO missing '*' separator".into()))?;
    Ok(format!("{}:{}", code, tokens[1]))
}

pub fn has_more_pages(response: &str) -> bool {
    response
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .map_or(false, |l| {
            let t = l.trim();
            t == ")>" || t == ")&gt;"
        })
}

pub struct TicketCsvRow {
    pub airline: String,
    pub ticket_no: String,
    pub doc_type: String,
    pub name: String,
    pub issue_date: String,
    pub route: String,
    pub tour_code: String,
    pub basic: String,
    pub total: String,
    pub fop: String,
    pub sign: String,
}

pub fn format_csv_row(row: &TicketCsvRow) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        row.sign,
        row.airline,
        row.ticket_no,
        row.doc_type,
        row.name,
        row.issue_date,
        row.route,
        row.tour_code,
        row.basic,
        row.total,
        row.fop,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const TJQ_XA: &str =
        "012576*077 6901233301   2339.00 881.00   0.00   0.00 XA HANNA/SA AS 88P123 TKTT";
    const TJQ_CA: &str =
        "012577*077 1932123561   2500.00   0.00   0.00   0.00 CA HANNA/SA AS 88P123 TKTT";
    const TJQ_EMDS: &str =
        "012578*235 6908222302  35550.80 21584T   0.00 139.66 CA ATTA/KHA AE 7TX123 EMDS";
    const TJQ_CC: &str =
        "012579*176 1234567890   1500.00 100.00   0.00   0.00 CC JOHN/DOE AS 1A2B3C TKTT";

    const TWD: &str = "TKT-0712446324401        RCI-                     1A  LOC-83428J\n\
        OD-CAICAI  SI-      FCPI-N   POI-CAI  DOI-19JUL26  IOI-90202092\n\
          1.HANNA/SAMER MR            ADT            ST\n\
        1 OCAI MS 953   S 03AUG0350 OK SRIEG/PV      O   03AUG03AUG 2PC\n\
        2 OHGH MS 954   S 20AUG0005 OK SRIEG/PV      O   20AUG20AUG 2PC\n\
           CAI\n\
        FARE   Y               IT\n\
        TOTALTAX EGP       881.00\n\
        TAXES    PD\n\
        TOTAL                  IT\n\
        /FC CAI MS HGH M/IT MS CAI M/IT END\n\
        FE EGP3000.00 NONREF\n\
        FO 077-6908236368CAI08JUL26/90202092/077-6908236368\n\
        FP O/CASH+/CASH\n\
        FT EVA";

    const EWD: &str = "EMD-0712451325261     TYPE-S                                 SYS-1A  LOC-88248J \n\
        INT-I          FCI-0  0         POI-CAI        DOI-19JUL26         IOI-92341292 \n\
        PAX- HANNA/SAMER MR                                                         ADT \n\
        RFIC-D  FINANCIAL IMPACT                                                    \n\
        REMARKS-                                                                      \n\
        CPN-1  RFISC-992  MS         S-F   SAC- 077XIM8WPGKAH  VALUE-2500.00          \n\
         DESCRIPTION-TICKET AMENDMENT FEE                                             \n\
         NON-REFUNDABLE                                                               \n\
         NON-EXCHANGEABLE                                                             \n\
         CONSUMED AT ISSUANCE                                                         \n\
         PRESENT TO-EGYPTAIR                                                          \n\
         PRESENT AT-CAIRO                                                             \n\
         ICW-0712343436401                                                            \n\
         SERVICE REMARKS-                                                             \n\
        FARE   F    EGP        2500.00                                                \n\
        EXCH VAL EGP    2500.00      RFND VAL                                         \n\
        TAX-                                                                          \n\
        TOTAL       EGP        2500.00                                                \n\
        /FC                                                                           \n\
        FP CASH                                                                       \n\
        FOID-";

    #[test]
    fn test_extract_airline() {
        assert_eq!(extract_airline(TJQ_XA).unwrap(), "MS");
        assert_eq!(extract_airline(TJQ_EMDS).unwrap(), "TK");
    }

    #[test]
    fn test_extract_ticket_no() {
        assert_eq!(extract_ticket_no(TJQ_XA).unwrap(), "6901233301");
        assert_eq!(extract_ticket_no(TJQ_EMDS).unwrap(), "6908222302");
    }

    #[test]
    fn test_extract_doc_type_reissue() {
        assert_eq!(extract_doc_type(TJQ_XA).unwrap(), "Reissue");
    }

    #[test]
    fn test_extract_doc_type_ticket() {
        assert_eq!(extract_doc_type(TJQ_CA).unwrap(), "Ticket");
    }

    #[test]
    fn test_extract_doc_type_emd() {
        assert_eq!(extract_doc_type(TJQ_EMDS).unwrap(), "EMD");
    }

    #[test]
    fn test_extract_name() {
        assert_eq!(extract_name(TJQ_XA).unwrap(), "HANNA/SA");
        assert_eq!(extract_name(TJQ_EMDS).unwrap(), "ATTA/KHA");
    }

    #[test]
    fn test_extract_name_from_twd_detail() {
        assert_eq!(
            extract_name_from_detail(TWD).unwrap(),
            "HANNA/SAMER MR"
        );
    }

    #[test]
    fn test_extract_name_from_ewd_detail() {
        assert_eq!(
            extract_name_from_detail(EWD).unwrap(),
            "HANNA/SAMER MR"
        );
    }

    #[test]
    fn test_extract_issue_date() {
        assert_eq!(extract_issue_date("19jul"), "19JUL");
        assert_eq!(extract_issue_date("23JUL"), "23JUL");
    }

    #[test]
    fn test_extract_route() {
        assert_eq!(extract_route(TWD).unwrap(), "CAI-HGH-CAI");
    }

    #[test]
    fn test_airport_to_city() {
        assert_eq!(airport_to_city("CAI").as_deref(), Some("Cairo"));
        assert!(airport_to_city("ZZZ").is_none());
    }

    #[test]
    fn test_route_to_cities() {
        assert_eq!(route_to_cities("CAI-HGH-CAI"), "Cairo-Hangzhou-Cairo");
        assert_eq!(route_to_cities("LHR-JFK"), "London-New York");
    }

    #[test]
    fn test_extract_route_with_transfers() {
        let twd = "TKT-2323141426402        RCI-                     1A  LOC-734226\n\
            OD-CAICAI  SI-      FCPI-F   POI-CAI  DOI-19JUL26  IOI-90125392\n\
              1.ATTA/KHALEDMOHAMED        ADT            ST\n\
            1 OCAI TK 693   T 09NOV0525 OK TCOORP3/FS03  O              3PC\n\
            2 XIST TK1555   T 09NOV1550 OK TCOORP3/FS03  O              3PC\n\
            3 OHAJ TK1556   T 13NOV1805 OK TCOORP3/FS03  O   11NOV      3PC\n\
            4 XIST TK 692   T 14NOV0215 OK TCOORP3/FS03  O   11NOV      3PC\n\
               CAI\n\
            FARE   F USD       276.00\n\
            EQUIV    EGP     13966.00       BSR        50.60\n\
            TOTALTAX EGP     21584.80\n\
            TOTAL    EGP     35550.80\n\
            /FC CAI TK X/IST TK HAJ138.22TK X/IST TK CAI138.22NUC276.44END R\n\
            OE1.00\n\
            FE NONEND/TK ONLY\n\
            FP CASH\n\
            FT CCC62377";
        assert_eq!(extract_route(twd).unwrap(), "CAI-IST-HAJ-IST-CAI");
        assert_eq!(
            route_to_cities(&extract_route(twd).unwrap()),
            "Cairo-Istanbul-Hannover-Istanbul-Cairo"
        );
    }

    #[test]
    fn test_extract_tour_code() {
        assert_eq!(extract_tour_code(TWD).unwrap(), "EVA");
    }

    #[test]
    fn test_extract_basic() {
        assert_eq!(extract_basic(TJQ_XA).unwrap(), "1458.00");
    }

    #[test]
    fn test_extract_basic_with_tax_string() {
        let tjq = "012578*235 6908222302  35550.80 21584T   0.00 139.66 CA ATTA/KHA AE 7TX123 TKTT";
        assert_eq!(extract_basic(tjq).unwrap(), "13966.80");
    }

    #[test]
    fn test_extract_basic_from_twd_no_equiv() {
        let twd = "TKT-0712446324401        RCI-                     1A  LOC-83428J\n\
                    OD-CAICAI  SI-      FCPI-N   POI-CAI  DOI-19JUL26  IOI-90202092\n\
                      1.HANNA/SAMER MR            ADT            ST\n\
                    1 OCAI MS 953   S 03AUG0350 OK SRIEG/PV      O   03AUG03AUG 2PC\n\
                    2 OHGH MS 954   S 20AUG0005 OK SRIEG/PV      O   20AUG20AUG 2PC\n\
                       CAI\n\
                    FARE   Y               IT\n\
                    TOTALTAX EGP       881.00\n\
                    TAXES    PD\n\
                    TOTAL                  IT\n\
                    /FC CAI MS HGH M/IT MS CAI M/IT END\n\
                    FE EGP3000.00 NONREF\n\
                    FO 077-6908236368CAI08JUL26/90202092/077-6908236368\n\
                    FP O/CASH+/CASH\n\
                    FT EVA";
        assert!(extract_basic_from_twd(twd).is_err());
    }

    #[test]
    fn test_extract_basic_from_twd_it_fare_with_bsr() {
        let twd = "TKT-0776908236405        RCI-                     1A  LOC-8COSFX\n\
                    OD-AMSCAI  SI-      FCPI-N   POI-CAI  DOI-20JUL26  IOI-90202092\n\
                      1.KORESSA/NESRINE MRS       ADT            ST\n\
                    1 OAMS MS 758   E 13SEP1540 OK ERIMSO/PV     O   13SEP13SEP 2PC\n\
                       CAI\n\
                    FARE   I               IT\n\
                    EQUIV                           BSR  57.96706247\n\
                    TOTALTAX EGP     10862.00\n\
                    TOTAL                  IT\n\
                    /FC AMS MS CAI Q50.00 M/IT  Q AMSCAI5.78END ROE0.86473\n\
                    FE NOT VALID FOR HAJ AND UMRA\n\
                    FP CASH\n\
                    FT EVA\n\
                    NON-ENDORSABLE\n\
                    FOR TAX/FEE DETAILS USE TWD/TAX\n\
                    NET REPORTING IT/BT";
        assert!(extract_basic_from_twd(twd).is_err());
    }

    #[test]
    fn test_extract_basic_from_twd_with_equiv() {
        let twd = "TKT-2201231236404        RCI-                     1A  LOC-8COSFX\n\
                    OD-CAIAMS  SI-      FCPI-0   POI-CAI  DOI-20JUL26  IOI-90212342\n\
                      1.KORRFFA/NJKKINE MRS       ADT            ST\n\
                    1 OCAI LH 585   H 06SEP0440 OK HXOXIYNC      O   06SEP06SEP 1PC\n\
                    2 XFRA LH 992   H 06SEP1250 OK HXOXIYNC      O   06SEP06SEP 1PC\n\
                       AMS\n\
                    FARE   F USD       462.00\n\
                    EQUIV    EGP     23378.00       BSR        50.60\n\
                    TOTALTAX EGP      7932.80\n\
                    TOTAL    EGP     31310.80\n\
                    /FC CAI LH X/FRA LH AMS462.00NUC462.00END ROE1.00\n\
                    FE FARE RESTRICTION MAY APPLY\n\
                    FP CASH\n\
                    FOR TAX/FEE DETAILS USE TWD/TAX";
        assert_eq!(extract_basic_from_twd(twd).unwrap(), "23378.00");
    }

    #[test]
    fn test_extract_basic_from_twd_equiv_with_non_numeric_value() {
        let twd = "TKT-0656908226439        RCI-                     1A  LOC-8E87FD\n\
                    OD-CAICAI  SI-      FCPI-0   POI-CAI  DOI-26JUN26  IOI-90289892\n\
                      1.HABASHI/WAHID MR          ADT            ST\n\
                    1 OCAI SV 322   B 27JUL0445 OK BARTEGB4      F   27JUL27JUL 1PC\n\
                    2 ORUH SV 417   L 28JUL1150 OK LARTEGB4      A   28JUL28JUL 1PC\n\
                       CAI\n\
                    FARE   R USD       328.00\n\
                    EQUIV    EGP          EGP       BSR        51.40\n\
                    TOTALTAX EGP       250.80\n\
                    TAXES    PD\n\
                    TOTAL    EGP      7960.80A\n\
                    /FC CAI SV RUH175.50SV CAI Q85.00 67.50NUC328.00END ROE1.00\n\
                    FE REC0SV - EGP5565.00 NONREF - TKT VALD 1Y FRM ISSUE DATE\n\
                    FO 065-6901116408CAI21JUN26/90202092/065-6923236408\n\
                    FP O/CA+/CASH\n\
                    FOR TAX/FEE DETAILS USE TWD/TAX\n\
                    SAC- 065R9JHZSQZK3";
        assert!(extract_basic_from_twd(twd).is_err());
    }

    #[test]
    fn test_extract_total() {
        assert_eq!(extract_total(TJQ_XA).unwrap(), "2339.00");
        assert_eq!(extract_total(TJQ_EMDS).unwrap(), "35550.80");
    }

    #[test]
    fn test_extract_fop_cash() {
        assert_eq!(extract_fop(TJQ_XA).unwrap(), "Cash");
        assert_eq!(extract_fop(TJQ_EMDS).unwrap(), "Cash");
    }

    #[test]
    fn test_extract_fop_cc() {
        assert_eq!(extract_fop(TJQ_CC).unwrap(), "CC");
    }

    #[test]
    fn test_extract_sign() {
        assert_eq!(extract_sign(TJQ_XA).unwrap(), "Amira");
        assert_eq!(extract_sign(TJQ_EMDS).unwrap(), "Amal");
    }

    #[test]
    fn test_map_airline() {
        assert_eq!(map_airline("077").unwrap(), "MS");
        assert_eq!(map_airline("235").unwrap(), "TK");
        assert!(map_airline("999").is_err());
    }

    #[test]
    fn test_map_doc_type() {
        assert_eq!(map_doc_type("TKTT", "XA"), "Reissue");
        assert_eq!(map_doc_type("TKTT", "CA"), "Ticket");
        assert_eq!(map_doc_type("RFND", "XA"), "Refund");
        assert_eq!(map_doc_type("CANX", "CA"), "Void");
        assert_eq!(map_doc_type("CANN", "CA"), "Void");
        assert_eq!(map_doc_type("EMDA", "CA"), "EMD");
        assert_eq!(map_doc_type("EMDS", "CA"), "EMD");
        assert_eq!(map_doc_type("OTHER", "CA"), "OTHER");
    }

    #[test]
    fn test_map_fop() {
        assert_eq!(map_fop("XA"), "Cash");
        assert_eq!(map_fop("CA"), "Cash");
        assert_eq!(map_fop("CC"), "CC");
        assert_eq!(map_fop("CX"), "CX");
    }

    #[test]
    fn test_map_sign() {
        assert_eq!(map_sign("AE").unwrap(), "Amal");
        assert_eq!(map_sign("AS").unwrap(), "Amira");
        assert_eq!(map_sign("MA").unwrap(), "Noha");
        assert_eq!(map_sign("SH").unwrap(), "Saeed");
        assert!(map_sign("XX").is_err());
    }

    #[test]
    fn test_parse_tjq_data_lines() {
        let response = "AGY NO - 90123123              QUERY REPORT 19JUL                  CURRENCY EGP\n\
                        OFFICE - C52341513             SELECTION:\n\
                        AGENT  - ALL                                                        20 JUL 2026\n\
                        -------------------------------------------------------------------------------\n\
                        SEQ NO A/L DOC NUMBER TOTAL DOC    TAX    FEE   COMM FP PAX NAME AS RLOC   TRNC\n\
                        -------------------------------------------------------------------------------\n\
                        012576*077 6901233301   2339.00 881.00   0.00   0.00 XA HANNA/SA AS 88P123 TKTT\n\
                        012578*235 6908222302  35550.80 21584T   0.00 139.66 CA ATTA/KHA AE 7TX123 TKTT\n";
        let lines = parse_tjq_data_lines(response);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("6901233301"));
    }

    #[test]
    fn test_parse_tjq_data_lines_filters_cnj() {
        let response = "-------------------------------------------------------------------------------\n\
                        SEQ NO A/L DOC NUMBER TOTAL DOC    TAX    FEE   COMM FP PAX NAME AS RLOC   TRNC\n\
                        -------------------------------------------------------------------------------\n\
                        012530*235 6912345369  34490.80 15270T   0.00 192.20 CA ABOUZEID AE 8OWC7E TKTT\n\
                               235 6912345370                                                       CNJ\n\
                        012531*235 1912345285   3016.00   0.00   0.00   0.00 CA ABOUZEID AE 8OWC7E EMDA\n";
        let lines = parse_tjq_data_lines(response);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("6912345369"));
        assert!(lines[1].contains("1912345285"));
    }

    #[test]
    fn test_extract_ticket_key() {
        assert_eq!(
            extract_ticket_key(TJQ_XA).unwrap(),
            "077:6901233301"
        );
        assert_eq!(
            extract_ticket_key(TJQ_EMDS).unwrap(),
            "235:6908222302"
        );
    }

    #[test]
    fn test_parse_tjq_data_lines_md_response() {
        let response = "012534*235 1912345288   3016.00   0.00   0.00   0.00 CA METWALLY AE 8EVP8X EMDA\n\
                        012535*235 6912345373  34490.80 15270T   0.00 192.20 CA METWALLY AE 8EVP8X TKTT\n\
                               235 6912345374                                                       CNJ\n\
                        012536*235 1912345289   3016.00   0.00   0.00   0.00 CA METWALLY AE 8EVP8X EMDA\n";
        let lines = parse_tjq_data_lines(response);
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("1912345288"));
        assert!(lines[1].contains("6912345373"));
        assert!(lines[2].contains("1912345289"));
    }

    #[test]
    fn test_parse_tjq_data_lines_md_response_with_separator() {
        let response = "SEQ NO A/L DOC NUMBER TOTAL DOC    TAX    FEE   COMM FP PAX NAME AS RLOC   TRNC\n\
                        -------------------------------------------------------------------------------\n\
                        012643*077 1951947193   2500.00   0.00   0.00   0.00 CA NABASIRY SH 8TPILL CANX\n\
                        012645*220 6908236448  26514.80 7839.T   0.00   0.00 CA SAROFIM/ AE 8DASC5 TKTT";
        let lines = parse_tjq_data_lines(response);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("1951947193"));
        assert!(lines[1].contains("6908236448"));
    }

    #[test]
    fn test_compact_void_line_with_blank_fop() {
        let line = "012635*077 1951947181      0.00   0.00   0.00   0.00    ABOUELDA SH 8TPILL CANX";
        assert_eq!(extract_ticket_key(line).unwrap(), "077:1951947181");
        assert_eq!(extract_doc_type(line).unwrap(), "Void");
        assert_eq!(extract_fop(line).unwrap(), "");
        assert_eq!(extract_sign(line).unwrap(), "Saeed");
    }

    #[test]
    fn test_has_more_pages() {
        assert!(has_more_pages("SOME DATA\n)>"));
        assert!(has_more_pages("SOME DATA\n)&gt;"));
        assert!(!has_more_pages("SOME DATA\nEND"));
        assert!(!has_more_pages("SOME DATA\n"));
        assert!(!has_more_pages(""));
    }

    #[test]
    fn test_format_csv_row() {
        let row = TicketCsvRow {
            airline: "MS".into(),
            ticket_no: "6901233301".into(),
            doc_type: "Reissue".into(),
            name: "HANNA/SA".into(),
            issue_date: "19JUL".into(),
            route: "CAI-HGH-CAI".into(),
            tour_code: "EVA".into(),
            basic: "1458.00".into(),
            total: "2339.00".into(),
            fop: "Cash".into(),
            sign: "Amira".into(),
        };
        assert_eq!(
            format_csv_row(&row),
            "Amira\tMS\t6901233301\tReissue\tHANNA/SA\t19JUL\tCAI-HGH-CAI\tEVA\t1458.00\t2339.00\tCash"
        );
    }

    #[test]
    fn test_extract_airline_code() {
        assert_eq!(extract_airline_code(TJQ_XA).unwrap(), "077");
        assert_eq!(extract_airline_code(TJQ_EMDS).unwrap(), "235");
    }

    #[test]
    fn test_tour_code_not_found() {
        let resp = "SOME OTHER DATA\nNO FT HERE\n";
        assert!(extract_tour_code(resp).is_err());
    }

    #[test]
    fn test_route_not_found() {
        let resp = "NO ITINERARY DATA\n";
        assert!(extract_route(resp).is_err());
    }
}
