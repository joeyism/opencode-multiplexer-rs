use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedProcess {
    pub pid: u32,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedServeProcess {
    pub pid: u32,
    pub port: u16,
}

pub fn scan_processes() -> anyhow::Result<Vec<ParsedProcess>> {
    let output = Command::new("ps").args(["-eo", "pid,args"]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().filter_map(parse_process_line).collect())
}

pub fn scan_serve_processes() -> anyhow::Result<Vec<ParsedServeProcess>> {
    let output = Command::new("ps").args(["-eo", "pid,args"]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter_map(parse_serve_process_line)
        .collect())
}

fn is_opencode_binary(token: &str) -> bool {
    let basename = token.rsplit('/').next().unwrap_or(token);
    basename == "opencode" || basename == ".opencode"
}

pub fn parse_serve_process_line(line: &str) -> Option<ParsedServeProcess> {
    let mut parts = line.split_whitespace();
    let pid: u32 = parts.next()?.parse().ok()?;
    let tokens: Vec<&str> = parts.collect();
    if tokens.is_empty() {
        return None;
    }

    let opencode_index = if is_opencode_binary(tokens[0]) {
        Some(0)
    } else if matches!(tokens[0], "node" | "bun" | "deno")
        && tokens.get(1).is_some_and(|token| is_opencode_binary(token))
    {
        Some(1)
    } else {
        None
    }?;

    if tokens.get(opencode_index + 1) != Some(&"serve") {
        return None;
    }

    for window in tokens[opencode_index + 2..].windows(2) {
        if window[0] == "--port" {
            let port = window[1].parse().ok()?;
            return Some(ParsedServeProcess { pid, port });
        }
    }

    None
}

pub fn parse_process_line(line: &str) -> Option<ParsedProcess> {
    let mut parts = line.split_whitespace();
    let pid: u32 = parts.next()?.parse().ok()?;
    let tokens: Vec<&str> = parts.collect();
    if tokens.is_empty() {
        return None;
    }

    let opencode_index = if is_opencode_binary(tokens[0]) {
        Some(0)
    } else if matches!(tokens[0], "node" | "bun" | "deno")
        && tokens.get(1).is_some_and(|token| is_opencode_binary(token))
    {
        Some(1)
    } else {
        None
    }?;

    if tokens.get(opencode_index + 1) == Some(&"serve") {
        return None;
    }

    let mut session_id = None;
    for window in tokens[opencode_index + 1..].windows(2) {
        if window[0] == "-s" {
            session_id = Some(window[1].to_string());
            break;
        }
    }

    Some(ParsedProcess { pid, session_id })
}
