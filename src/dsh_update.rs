use super::*;

pub(super) fn source_version(paths: &Paths) -> Result<String, String> {
    let output = run_capture(
        paths,
        hidden_command("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                include_str!("dsh_releases.ps1"),
            ])
            .env("TEMP", &paths.temp)
            .env("TMP", &paths.temp),
        "查询 DSH 源码发布",
        LAUNCHER_QUERY_TIMEOUT,
        true,
    )?;
    parse_source_version(&output).ok_or_else(|| "官方源码发布列表中没有有效 DSH 版本".to_owned())
}

fn parse_source_version(text: &str) -> Option<String> {
    let releases: serde_json::Value = serde_json::from_str(text).ok()?;
    releases
        .as_array()?
        .iter()
        .filter(|release| release["draft"].as_bool() == Some(false))
        .filter_map(|release| release["tag_name"].as_str()?.strip_prefix("dsh-v"))
        .filter_map(|text| parse_version(text).map(|version| (version, text)))
        .max_by(|left, right| left.0.cmp(&right.0))
        .map(|(_, text)| text.to_owned())
}

pub(super) fn source_notice(npm: &str, source: Result<&str, &str>) -> Option<String> {
    match source {
        Ok(source) if parse_version(source)? > parse_version(npm)? => {
            Some(format!("源码 {source} 已发布，npm 尚未提供"))
        }
        Ok(_) => None,
        Err(_) => Some("源码版本未能核实；本次仅核对 npm 可安装版本".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_releases_include_prereleases_and_ignore_other_packages_and_drafts() {
        assert_eq!(
            parse_source_version(
                r#"[
            {"tag_name":"dsh-v0.1.3-alpha.1","draft":false},
            {"tag_name":"dsh-v0.1.2-rc.1","draft":false},
            {"tag_name":"sdk-v9.0.0","draft":false},
            {"tag_name":"dsh-v8.0.0","draft":true},
            {"tag_name":"dsh-v7.0.0"},
            {"tag_name":"dsh-vbad","draft":false}
        ]"#
            ),
            Some("0.1.3-alpha.1".to_owned())
        );
        assert!(parse_source_version(r#"{"message":"API rate limit exceeded"}"#).is_none());
        assert!(parse_source_version("[]").is_none());
    }

    #[test]
    fn source_only_release_is_not_advertised_as_installable() {
        let notice = source_notice("0.1.2-rc.1", Ok("0.1.3-alpha.1")).unwrap();
        assert!(notice.contains("源码 0.1.3-alpha.1 已发布，npm 尚未提供"));
        assert!(source_notice("0.1.3-alpha.1", Ok("0.1.3-alpha.1")).is_none());
        assert!(source_notice("0.1.3", Ok("0.1.3-alpha.1")).is_none());
        assert!(source_notice("0.1.2-rc.1", Err("timeout"))
            .unwrap()
            .contains("未能核实"));
    }
}
