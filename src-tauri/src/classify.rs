//! Works out what a process actually is from its name, path and arguments.
//!
//! Most dev tooling on Windows hides behind generic names. `node.exe` can be Vite
//! or an MCP server, and `Code.exe` runs language servers as plain Node scripts
//! with no `--type` flag at all, so the script argument matters more than the name.

use crate::model::Category;

#[derive(Debug, Clone, PartialEq)]
pub struct Class {
    pub category: Category,
    pub role: String,
    pub tech: Option<String>,
}

fn class(category: Category, role: &str, tech: Option<&str>) -> Class {
    Class {
        category,
        role: role.to_string(),
        tech: tech.map(str::to_string),
    }
}

// Names without .exe, so one table serves every OS.
const EDITORS: &[(&str, &str)] = &[
    ("code", "VS Code"),
    ("code - insiders", "VS Code Insiders"),
    ("code-insiders", "VS Code Insiders"),
    ("cursor", "Cursor"),
    ("windsurf", "Windsurf"),
    ("zed", "Zed"),
    ("zeditor", "Zed"),
    ("devenv", "Visual Studio"),
    ("rider64", "Rider"),
    ("rider", "Rider"),
    ("webstorm64", "WebStorm"),
    ("webstorm", "WebStorm"),
    ("idea64", "IntelliJ IDEA"),
    ("idea", "IntelliJ IDEA"),
    ("pycharm64", "PyCharm"),
    ("pycharm", "PyCharm"),
    ("phpstorm64", "PhpStorm"),
    ("phpstorm", "PhpStorm"),
    ("goland64", "GoLand"),
    ("goland", "GoLand"),
    ("sublime_text", "Sublime Text"),
    ("notepad++", "Notepad++"),
    ("nvim", "Neovim"),
    ("vim", "Vim"),
    ("emacs", "Emacs"),
    ("hx", "Helix"),
    ("unrealeditor", "Unreal Editor"),
    ("godot", "Godot"),
];

const BROWSERS: &[(&str, &str)] = &[
    ("chrome", "Chrome"),
    ("chromium", "Chromium"),
    ("chromium-browser", "Chromium"),
    ("msedge", "Edge"),
    ("brave", "Brave"),
    ("firefox", "Firefox"),
    ("firefox-bin", "Firefox"),
    ("firefox-esr", "Firefox"),
    ("opera", "Opera"),
    ("vivaldi", "Vivaldi"),
    ("vivaldi-bin", "Vivaldi"),
    ("arc", "Arc"),
];

const NATIVE_LSP: &[(&str, &str)] = &[
    ("rust-analyzer", "Rust Analyzer"),
    ("gopls", "gopls"),
    ("clangd", "clangd"),
    ("omnisharp", "OmniSharp"),
    ("lua-language-server", "Lua LS"),
    ("tsgo", "TypeScript Go"),
    ("biome", "Biome"),
    ("ruff", "Ruff"),
    ("taplo", "Taplo"),
];

const NATIVE_DEV: &[(&str, Category, &str, &str)] = &[
    ("postgres", Category::Database, "PostgreSQL server", "PostgreSQL"),
    ("mysqld", Category::Database, "MySQL server", "MySQL"),
    ("mariadbd", Category::Database, "MariaDB server", "MariaDB"),
    ("mongod", Category::Database, "MongoDB server", "MongoDB"),
    ("redis-server", Category::Database, "Redis server", "Redis"),
    ("memurai", Category::Database, "Memurai server", "Redis"),
    ("sqlservr", Category::Database, "SQL Server", "SQL Server"),
    ("docker desktop", Category::Container, "Docker Desktop", "Docker"),
    ("dockerd", Category::Container, "Docker daemon", "Docker"),
    ("docker", Category::Container, "Docker CLI", "Docker"),
    ("com.docker.backend", Category::Container, "Docker backend", "Docker"),
    ("com.docker.build", Category::Container, "Docker build", "Docker"),
    ("com.docker.service", Category::Container, "Docker service", "Docker"),
    ("vmmem", Category::Container, "WSL or Hyper-V VM memory", "WSL"),
    ("vmmemwsl", Category::Container, "WSL VM memory", "WSL"),
    ("wsl", Category::Container, "WSL launcher", "WSL"),
    ("wslhost", Category::Container, "WSL host", "WSL"),
    ("wslrelay", Category::Container, "WSL network relay", "WSL"),
    ("wslservice", Category::Container, "WSL service", "WSL"),
    ("podman", Category::Container, "Podman", "Podman"),
    ("kubectl", Category::Container, "kubectl", "Kubernetes"),
    ("containerd", Category::Container, "containerd runtime", "Docker"),
    ("containerd-shim-runc-v2", Category::Container, "Container shim", "Docker"),
    ("nginx", Category::DevServer, "nginx server", "nginx"),
    ("caddy", Category::DevServer, "Caddy server", "Caddy"),
    ("httpd", Category::DevServer, "Apache server", "Apache"),
    ("ngrok", Category::DevServer, "ngrok tunnel", "ngrok"),
    ("cloudflared", Category::DevServer, "Cloudflare tunnel", "Cloudflare"),
    ("php", Category::Runtime, "PHP process", "PHP"),
    ("php-cgi", Category::Runtime, "PHP FastCGI worker", "PHP"),
    ("php-fpm", Category::Runtime, "PHP FastCGI worker", "PHP"),
    ("ruby", Category::Runtime, "Ruby process", "Ruby"),
    ("java", Category::Runtime, "Java process", "Java"),
    ("javaw", Category::Runtime, "Java process", "Java"),
    ("dotnet", Category::Runtime, ".NET process", ".NET"),
    ("esbuild", Category::BuildTool, "esbuild service", "esbuild"),
    ("make", Category::BuildTool, "Make", "C++"),
    ("gcc", Category::BuildTool, "GCC compiler", "C++"),
    ("clang", Category::BuildTool, "Clang compiler", "C++"),
    ("cargo", Category::BuildTool, "Cargo", "Rust"),
    ("rustc", Category::BuildTool, "Rust compiler", "Rust"),
    ("msbuild", Category::BuildTool, "MSBuild", ".NET"),
    ("cl", Category::BuildTool, "MSVC compiler", "C++"),
    ("link", Category::BuildTool, "MSVC linker", "C++"),
    ("gradle", Category::BuildTool, "Gradle", "Java"),
    ("go", Category::BuildTool, "Go toolchain", "Go"),
    ("git", Category::VersionControl, "Git", "Git"),
    ("git-remote-https", Category::VersionControl, "Git network transfer", "Git"),
    ("gh", Category::VersionControl, "GitHub CLI", "GitHub"),
    ("ssh", Category::VersionControl, "SSH client", "SSH"),
    ("windowsterminal", Category::Terminal, "Windows Terminal", ""),
    ("openconsole", Category::Terminal, "Console host", ""),
    ("conhost", Category::Terminal, "Console host", ""),
    ("powershell", Category::Terminal, "PowerShell", ""),
    ("pwsh", Category::Terminal, "PowerShell", ""),
    ("cmd", Category::Terminal, "Command Prompt", ""),
    ("bash", Category::Terminal, "Bash", ""),
    ("sh", Category::Terminal, "Shell", ""),
    ("wezterm-gui", Category::Terminal, "WezTerm", ""),
    ("alacritty", Category::Terminal, "Alacritty", ""),
    ("zsh", Category::Terminal, "Zsh", ""),
    ("fish", Category::Terminal, "Fish", ""),
    ("dash", Category::Terminal, "Dash", ""),
    ("tmux: server", Category::Terminal, "Terminal multiplexer", ""),
    ("tmux", Category::Terminal, "Terminal multiplexer", ""),
    ("screen", Category::Terminal, "Terminal multiplexer", ""),
    ("gnome-terminal-server", Category::Terminal, "GNOME Terminal", ""),
    ("ptyxis", Category::Terminal, "Ptyxis", ""),
    ("konsole", Category::Terminal, "Konsole", ""),
    ("xterm", Category::Terminal, "xterm", ""),
    ("kitty", Category::Terminal, "kitty", ""),
    ("foot", Category::Terminal, "foot", ""),
    ("tilix", Category::Terminal, "Tilix", ""),
    ("terminator", Category::Terminal, "Terminator", ""),
    ("xfce4-terminal", Category::Terminal, "Xfce Terminal", ""),
];

/// Plain shells, as opposed to terminal windows and tmux, which outlive their launcher on purpose.
pub fn is_shell_role(role: &str) -> bool {
    matches!(role, "PowerShell" | "Command Prompt" | "Bash" | "Shell" | "Zsh" | "Fish" | "Dash")
}

/// Services and desktop plumbing on Linux. Ending these can log someone out.
const LINUX_SYSTEM_NAMES: &[&str] = &[
    "init", "kthreadd", "dbus-daemon", "dbus-broker", "dbus-broker-launch", "polkitd", "udisksd", "networkmanager",
    "pipewire", "pipewire-pulse", "wireplumber", "pulseaudio", "xorg", "xwayland", "gnome-shell", "plasmashell",
    "kwin_wayland", "kwin_x11", "mutter", "sway", "hyprland", "sddm", "gdm", "gdm3", "lightdm", "login", "agetty",
    "cron", "crond", "rsyslogd", "irqbalance", "snapd", "multipathd", "accounts-daemon", "upowerd", "modemmanager",
    "avahi-daemon", "bluetoothd", "sshd", "wpa_supplicant", "thermald", "auditd",
];

const LINUX_SYSTEM_PREFIXES: &[&str] = &["systemd", "gsd-", "gvfs", "ibus-", "xdg-", "at-spi", "gnome-session"];

const LINUX_SYSTEM_FOLDERS: &[&str] = &["/usr/lib/systemd/", "/lib/systemd/", "/usr/libexec/", "/usr/lib/polkit"];

const SYSTEM_NAMES: &[&str] = &[
    "system", "registry", "smss", "csrss", "wininit", "winlogon", "services", "lsass", "svchost",
    "fontdrvhost", "dwm", "memcompression", "memory compression", "sihost", "taskhostw",
    "runtimebroker", "searchhost", "searchindexer", "startmenuexperiencehost",
    "shellexperiencehost", "textinputhost", "ctfmon", "dllhost", "wudfhost", "spoolsv", "wmiprvse",
    "audiodg", "securityhealthservice", "securityhealthsystray", "msmpeng", "nissrv", "explorer",
    "applicationframehost", "lockapp", "unsecapp", "wlanext", "system idle process", "[system process]",
    "secure system", "lsaiso",
];

/// Scripts run by a JS runtime. Checked in order, first match wins, so more
/// specific needles have to sit above general ones.
const SCRIPTS: &[(&str, Category, &str, &str)] = &[
    ("typescript/lib/tsserver", Category::LanguageServer, "TypeScript server", "TypeScript"),
    ("typescript/lib/typingsinstaller", Category::LanguageServer, "TypeScript typings installer", "TypeScript"),
    ("typingsinstaller", Category::LanguageServer, "TypeScript typings installer", "TypeScript"),
    ("tsserver", Category::LanguageServer, "TypeScript server", "TypeScript"),
    ("html-language-features", Category::LanguageServer, "HTML language server", "HTML"),
    ("css-language-features", Category::LanguageServer, "CSS language server", "CSS"),
    ("json-language-features", Category::LanguageServer, "JSON language server", "JSON"),
    ("markdown-language-features", Category::LanguageServer, "Markdown language server", "Markdown"),
    ("@anthropic-ai/claude-code", Category::AiAgent, "Claude Code session", "Claude"),
    ("@openai/codex", Category::AiAgent, "Codex session", "Codex"),
    ("@google/gemini-cli", Category::AiAgent, "Gemini CLI session", "Gemini"),
    ("@modelcontextprotocol", Category::McpServer, "MCP server", "MCP"),
    ("mcp-server", Category::McpServer, "MCP server", "MCP"),
    ("-mcp/", Category::McpServer, "MCP server", "MCP"),
    ("-mcp.js", Category::McpServer, "MCP server", "MCP"),
    ("/mcp-", Category::McpServer, "MCP server", "MCP"),
    ("vitest", Category::TestRunner, "Vitest", "Vitest"),
    ("jest", Category::TestRunner, "Jest", "Jest"),
    ("playwright", Category::TestRunner, "Playwright", "Playwright"),
    ("cypress", Category::TestRunner, "Cypress", "Cypress"),
    ("mocha", Category::TestRunner, "Mocha", "Mocha"),
    ("storybook", Category::DevServer, "Storybook", "Storybook"),
    ("next/dist", Category::DevServer, "Next.js server", "Next.js"),
    ("next-server", Category::DevServer, "Next.js server", "Next.js"),
    (".bin/next", Category::DevServer, "Next.js server", "Next.js"),
    ("nuxt", Category::DevServer, "Nuxt server", "Nuxt"),
    ("astro", Category::DevServer, "Astro server", "Astro"),
    ("@remix-run", Category::DevServer, "Remix server", "Remix"),
    ("@sveltejs/kit", Category::DevServer, "SvelteKit server", "SvelteKit"),
    ("react-scripts", Category::DevServer, "Create React App server", "React"),
    ("webpack-dev-server", Category::DevServer, "webpack dev server", "webpack"),
    ("@angular/cli", Category::DevServer, "Angular dev server", "Angular"),
    ("gatsby", Category::DevServer, "Gatsby server", "Gatsby"),
    ("vue-cli-service", Category::DevServer, "Vue CLI server", "Vue"),
    ("parcel", Category::DevServer, "Parcel server", "Parcel"),
    ("wrangler", Category::DevServer, "Wrangler dev server", "Cloudflare"),
    ("netlify", Category::DevServer, "Netlify dev", "Netlify"),
    ("vercel", Category::DevServer, "Vercel dev", "Vercel"),
    ("supabase", Category::DevServer, "Supabase CLI", "Supabase"),
    ("firebase", Category::DevServer, "Firebase emulator", "Firebase"),
    ("vite", Category::DevServer, "Vite dev server", "Vite"),
    ("typescript/bin/tsc", Category::BuildTool, "TypeScript compiler", "TypeScript"),
    ("typescript/lib/tsc", Category::BuildTool, "TypeScript compiler", "TypeScript"),
    ("webpack", Category::BuildTool, "webpack", "webpack"),
    ("rollup", Category::BuildTool, "Rollup", "Rollup"),
    ("tsup", Category::BuildTool, "tsup", "tsup"),
    ("turbo", Category::BuildTool, "Turborepo", "Turborepo"),
    ("nx/", Category::BuildTool, "Nx", "Nx"),
    ("nodemon", Category::BuildTool, "nodemon watcher", "nodemon"),
    ("tsx", Category::BuildTool, "tsx runner", "tsx"),
    ("ts-node", Category::BuildTool, "ts-node runner", "ts-node"),
    ("concurrently", Category::BuildTool, "concurrently", "concurrently"),
    ("tailwindcss", Category::BuildTool, "Tailwind CLI", "Tailwind CSS"),
    ("postcss", Category::BuildTool, "PostCSS", "PostCSS"),
    ("prisma", Category::BuildTool, "Prisma", "Prisma"),
    ("drizzle-kit", Category::BuildTool, "Drizzle Kit", "Drizzle"),
    ("eslint", Category::BuildTool, "ESLint", "ESLint"),
    ("prettier", Category::BuildTool, "Prettier", "Prettier"),
    ("pm2", Category::Runtime, "PM2 daemon", "PM2"),
    ("npm-cli.js", Category::PackageManager, "npm", "npm"),
    ("npm/bin", Category::PackageManager, "npm", "npm"),
    ("npx-cli.js", Category::PackageManager, "npx", "npm"),
    ("pnpm", Category::PackageManager, "pnpm", "pnpm"),
    ("yarn", Category::PackageManager, "Yarn", "Yarn"),
    ("corepack", Category::PackageManager, "Corepack", "Corepack"),
];

/// Friendly names for VS Code extensions that run a server.
const EXTENSIONS: &[(&str, Category, &str, &str)] = &[
    ("ms-python.vscode-pylance", Category::LanguageServer, "Pylance language server", "Python"),
    ("ms-python.vscode-python-envs", Category::LanguageServer, "Python environment tools", "Python"),
    ("ms-python.python", Category::LanguageServer, "Python extension helper", "Python"),
    ("bradlc.vscode-tailwindcss", Category::LanguageServer, "Tailwind CSS language server", "Tailwind CSS"),
    ("dbaeumer.vscode-eslint", Category::LanguageServer, "ESLint server", "ESLint"),
    ("esbenp.prettier-vscode", Category::LanguageServer, "Prettier extension helper", "Prettier"),
    ("github.vscode-github-actions", Category::LanguageServer, "GitHub Actions language server", "GitHub Actions"),
    ("rust-lang.rust-analyzer", Category::LanguageServer, "Rust Analyzer", "Rust"),
    ("golang.go", Category::LanguageServer, "Go extension helper", "Go"),
    ("vue.volar", Category::LanguageServer, "Vue language server", "Vue"),
    ("svelte.svelte-vscode", Category::LanguageServer, "Svelte language server", "Svelte"),
    ("astro-build.astro-vscode", Category::LanguageServer, "Astro language server", "Astro"),
    ("prisma.prisma", Category::LanguageServer, "Prisma language server", "Prisma"),
    ("anthropic.claude-code", Category::AiAgent, "Claude Code session", "Claude"),
    ("github.copilot", Category::AiAgent, "GitHub Copilot", "Copilot"),
];

/// Pulls `ms-python.vscode-pylance` out of `.../.vscode/extensions/ms-python.vscode-pylance-2026.4.1/...`.
pub fn extension_id(path_lower: &str) -> Option<String> {
    for marker in [".vscode/extensions/", ".cursor/extensions/", ".windsurf/extensions/", ".vscode-insiders/extensions/"] {
        if let Some(i) = path_lower.find(marker) {
            let rest = &path_lower[i + marker.len()..];
            let folder = rest.split('/').next().unwrap_or(rest);
            // Trim the version: the last `-` that is followed by a digit.
            let mut end = folder.len();
            for (idx, _) in folder.match_indices('-') {
                if folder[idx + 1..].starts_with(|c: char| c.is_ascii_digit()) {
                    end = idx;
                    break;
                }
            }
            return Some(folder[..end].to_string());
        }
    }
    None
}

fn normal(s: &str) -> String {
    s.to_lowercase().replace('\\', "/")
}

fn stem(name_lower: &str) -> &str {
    name_lower.strip_suffix(".exe").unwrap_or(name_lower)
}

fn base_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// First argument that is not a flag, skipping the value of flags that take one.
pub fn script_arg(args: &[String]) -> Option<&str> {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg.starts_with('-') {
            if matches!(arg.as_str(), "-r" | "--require" | "--import" | "--loader" | "-e" | "--eval" | "-p" | "--print") {
                iter.next();
            }
            continue;
        }
        return Some(arg);
    }
    None
}

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let prefix = format!("{flag}=");
    args.iter().find_map(|a| a.strip_prefix(prefix.as_str()))
}

fn has_arg(args: &[String], word: &str) -> bool {
    args.iter().skip(1).any(|a| a == word)
}

/// Role of a Chromium or Electron child process, from its `--type` flag.
fn chromium_role(args: &[String], renderer_label: &str) -> Option<String> {
    let ty = flag_value(args, "--type")?;
    let role = match ty {
        "renderer" => renderer_label,
        "gpu-process" => "GPU process",
        "crashpad-handler" => "Crash reporter",
        "broker" => "Sandbox broker",
        "utility" => match flag_value(args, "--utility-sub-type").unwrap_or("") {
            "network.mojom.NetworkService" => "Network service",
            "storage.mojom.StorageService" => "Storage service",
            "audio.mojom.AudioService" => "Audio service",
            "video_capture.mojom.VideoCaptureService" => "Video capture service",
            "node.mojom.NodeService" => "Node helper",
            _ => "Utility process",
        },
        _ => "Helper process",
    };
    Some(role.to_string())
}

/// Classifies a script run by Node, Bun, Deno or Electron in Node mode.
fn classify_script(script: &str, args: &[String], runtime: &str) -> Class {
    let lower = normal(script);
    if let Some(ext) = extension_id(&lower) {
        // Built-in names like `typescript-next` carry tsserver, so check scripts first.
        if lower.contains("tsserver") || lower.contains("typingsinstaller") {
            return match_script(&lower, args).unwrap_or_else(|| generic_script(script, runtime));
        }
        if let Some(found) = extension_class(&ext) {
            return found;
        }
        return class(
            Category::LanguageServer,
            &format!("Extension server ({ext})"),
            None,
        );
    }
    if let Some(found) = match_script(&lower, args) {
        return found;
    }
    // Tools launched through `npx foo` or a bare `.bin` shim show up in later args.
    let joined = normal(&args.iter().skip(1).cloned().collect::<Vec<_>>().join(" "));
    if joined.contains("mcp") && (joined.contains("npx") || joined.contains("-mcp") || joined.contains("mcp-")) {
        return class(Category::McpServer, "MCP server", Some("MCP"));
    }
    generic_script(script, runtime)
}

fn match_script(lower: &str, args: &[String]) -> Option<Class> {
    for (needle, category, role, tech) in SCRIPTS {
        if lower.contains(needle) {
            // Vite and friends run in both dev and one shot build modes.
            if *needle == "vite" && has_arg(args, "build") {
                return Some(class(Category::BuildTool, "Vite build", Some("Vite")));
            }
            if *needle == "vite" && has_arg(args, "preview") {
                return Some(class(Category::DevServer, "Vite preview server", Some("Vite")));
            }
            if *needle == "typescript/bin/tsc" || *needle == "typescript/lib/tsc" {
                let role = if has_arg(args, "--watch") || has_arg(args, "-w") {
                    "TypeScript watcher"
                } else {
                    role
                };
                return Some(class(*category, role, Some(tech)));
            }
            if *needle == "webpack" && has_arg(args, "serve") {
                return Some(class(Category::DevServer, "webpack dev server", Some("webpack")));
            }
            return Some(class(*category, role, Some(tech)));
        }
    }
    None
}

fn extension_class(ext: &str) -> Option<Class> {
    EXTENSIONS
        .iter()
        .find(|(id, ..)| ext == *id || ext.starts_with(&format!("{id}-")))
        .map(|(_, category, role, tech)| class(*category, role, Some(tech)))
}

fn generic_script(script: &str, runtime: &str) -> Class {
    class(
        Category::Runtime,
        &format!("{runtime} script: {}", base_name(script)),
        Some(runtime),
    )
}

fn classify_python(args: &[String]) -> Class {
    let joined = normal(&args.iter().skip(1).cloned().collect::<Vec<_>>().join(" "));
    for (needle, role, tech) in [
        ("manage.py runserver", "Django dev server", "Django"),
        ("http.server", "Python HTTP server", "Python"),
        ("uvicorn", "Uvicorn server", "FastAPI"),
        ("gunicorn", "Gunicorn server", "Python"),
        ("flask", "Flask dev server", "Flask"),
        ("jupyter", "Jupyter server", "Jupyter"),
        ("streamlit", "Streamlit server", "Streamlit"),
        ("celery", "Celery worker", "Celery"),
    ] {
        if joined.contains(needle) {
            return class(Category::DevServer, role, Some(tech));
        }
    }
    if joined.contains("pytest") {
        return class(Category::TestRunner, "pytest", Some("pytest"));
    }
    if joined.contains("mcp") {
        return class(Category::McpServer, "MCP server", Some("MCP"));
    }
    if let Some(ext) = extension_id(&joined) {
        if let Some(found) = extension_class(&ext) {
            return found;
        }
    }
    if let Some(module) = args.iter().position(|a| a == "-m").and_then(|i| args.get(i + 1)) {
        return class(Category::Runtime, &format!("Python module: {module}"), Some("Python"));
    }
    match script_arg(args) {
        Some(s) => class(Category::Runtime, &format!("Python script: {}", base_name(s)), Some("Python")),
        None => class(Category::Runtime, "Python process", Some("Python")),
    }
}

/// The single entry point. Falls back to a best guess, it never fails.
pub fn classify(name: &str, exe: &str, args: &[String]) -> Class {
    let name_lower = name.to_lowercase();
    let stem_name = stem(&name_lower);
    let exe_lower = normal(exe);
    let exe_stem = stem(base_name(&exe_lower));
    let named = |n: &str| n == stem_name || n == exe_stem;

    // Native tools bundled inside an editor extension folder.
    if let Some(ext) = extension_id(&exe_lower) {
        if ext.starts_with("anthropic.claude-code") {
            return class(Category::AiAgent, "Claude Code session (editor)", Some("Claude"));
        }
        if let Some(found) = extension_class(&ext) {
            return found;
        }
        return class(Category::LanguageServer, &format!("Extension tool ({ext})"), None);
    }

    if stem_name == "copilot-runtime" {
        return class(Category::AiAgent, "GitHub Copilot runtime", Some("Copilot"));
    }

    if stem_name == "claude" {
        if flag_value(args, "--type").is_some() || exe_lower.contains("anthropicclaude") {
            let role = chromium_role(args, "Window or webview").unwrap_or_else(|| "Main process".into());
            return class(Category::AiAgent, &format!("Claude: {role}"), Some("Claude"));
        }
        return class(Category::AiAgent, "Claude Code session", Some("Claude"));
    }

    if let Some((_, editor)) = EDITORS.iter().find(|(n, _)| named(n)) {
        if flag_value(args, "--type").is_some() {
            let mut role = chromium_role(args, "Window or webview").unwrap_or_default();
            // VS Code only opens an inspector port on extension hosts.
            if role == "Node helper" && args.iter().any(|a| a.starts_with("--inspect-port")) {
                role = "Extension host".into();
            }
            return class(Category::Editor, &format!("{editor}: {role}"), Some(editor));
        }
        if let Some(script) = script_arg(args) {
            let looks_like_script = script.ends_with(".js")
                || script.ends_with(".cjs")
                || script.ends_with(".mjs")
                || normal(script).contains("/extensions/")
                || normal(script).contains("servermain")
                || normal(script).contains("serverworkermain");
            if looks_like_script {
                return classify_script(script, args, editor);
            }
        }
        return class(Category::Editor, &format!("{editor}: Main process"), Some(editor));
    }

    if let Some((_, browser)) = BROWSERS.iter().find(|(n, _)| named(n)) {
        // Firefox names its children differently from Chromium.
        let role = if has_arg(args, "-contentproc") {
            if has_arg(args, "-isForBrowser") { "Tab or extension".to_string() } else { "Utility process".to_string() }
        } else {
            chromium_role(args, "Tab or extension").unwrap_or_else(|| "Main process".into())
        };
        return class(Category::Browser, &format!("{browser}: {role}"), Some(browser));
    }
    if matches!(exe_stem, "webkitwebprocess" | "webkitnetworkprocess" | "webkitgpuprocess") {
        return class(Category::Browser, "WebKit: Web content", Some("WebKit"));
    }
    if stem_name == "msedgewebview2" {
        let role = chromium_role(args, "Web content").unwrap_or_else(|| "Main process".into());
        return class(Category::Browser, &format!("WebView2: {role}"), Some("WebView2"));
    }

    // Node tools on Linux can rewrite their process title, which hides the script.
    if stem_name.starts_with("next-server") {
        return class(Category::DevServer, "Next.js server", Some("Next.js"));
    }
    if stem_name == "npm" || stem_name.starts_with("npm ") {
        return class(Category::PackageManager, "npm", Some("npm"));
    }

    if matches!(stem_name, "node" | "bun" | "deno") {
        let runtime = match stem_name {
            "bun" => "Bun",
            "deno" => "Deno",
            _ => "Node.js",
        };
        return match script_arg(args) {
            Some(script) => classify_script(script, args, runtime),
            None => class(Category::Runtime, &format!("{runtime} process"), Some(runtime)),
        };
    }

    if stem_name.starts_with("python") || stem_name == "py" || stem_name == "pythonw" {
        return classify_python(args);
    }

    for (lsp, label) in NATIVE_LSP {
        if stem_name == *lsp {
            return class(Category::LanguageServer, &format!("{label} language server"), Some(label));
        }
    }

    for (native, category, role, tech) in NATIVE_DEV {
        if stem_name == *native {
            let tech = if tech.is_empty() { None } else { Some(*tech) };
            return class(*category, role, tech);
        }
    }

    // Coreutils from Git for Windows and MSYS: sleep, grep and friends.
    if exe_lower.contains("/git/usr/bin/") || exe_lower.contains("/git/mingw64/bin/") || exe_lower.contains("/msys64/") {
        return class(Category::Terminal, &format!("Shell command: {stem_name}"), None);
    }

    // Driver packages install under Windows but belong to their vendor.
    let in_windows = exe_lower.starts_with("c:/windows/") && !exe_lower.contains("/driverstore/");
    let linux_system = LINUX_SYSTEM_NAMES.contains(&stem_name)
        || LINUX_SYSTEM_PREFIXES.iter().any(|p| stem_name.starts_with(p))
        || LINUX_SYSTEM_FOLDERS.iter().any(|f| exe_lower.starts_with(f));
    if SYSTEM_NAMES.contains(&stem_name) || in_windows || linux_system {
        return class(Category::System, crate::platform::SYSTEM_ROLE, None);
    }

    // Any other Electron or Chromium based app still reports its child type.
    if let Some(role) = chromium_role(args, "Window or webview") {
        return class(Category::App, &role, None);
    }

    if exe.is_empty() {
        // Only kernel threads have no executable and no command line on Linux.
        if cfg!(target_os = "linux") && args.is_empty() {
            return class(Category::System, "Kernel thread", None);
        }
        return class(Category::Other, "Process details not readable", None);
    }
    class(Category::App, "Application", None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    const CODE: &str = r"C:\Users\me\AppData\Local\Programs\Microsoft VS Code\Code.exe";

    #[test]
    fn vs_code_runs_tsserver_as_a_plain_node_script() {
        let c = classify(
            "Code.exe",
            CODE,
            &args(&[CODE, "--max-old-space-size=3072", r"c:\Users\me\.vscode\extensions\ms-vscode.vscode-typescript-next-6.0.20260416\node_modules\typescript\lib\tsserver.js"]),
        );
        assert_eq!(c.category, Category::LanguageServer);
        assert_eq!(c.role, "TypeScript server");
    }

    #[test]
    fn vs_code_extension_servers_get_friendly_names() {
        let c = classify(
            "Code.exe",
            CODE,
            &args(&[CODE, r"c:\Users\me\.vscode\extensions\ms-python.vscode-pylance-2026.4.1\dist\server.bundle.js", "--cancellationReceive=file:abc"]),
        );
        assert_eq!(c.role, "Pylance language server");
        let c = classify(
            "Code.exe",
            CODE,
            &args(&[CODE, r"c:\Users\me\.vscode\extensions\bradlc.vscode-tailwindcss-0.16.0\dist\tailwindServer.js", "--node-ipc"]),
        );
        assert_eq!(c.role, "Tailwind CSS language server");
    }

    #[test]
    fn vs_code_builtin_servers_are_named() {
        let c = classify(
            "Code.exe",
            CODE,
            &args(&[CODE, r"C:\Users\me\AppData\Local\Programs\Microsoft VS Code\2c33b50b16\resources\app\extensions\html-language-features\server\dist\node\htmlServerMain"]),
        );
        assert_eq!(c.role, "HTML language server");
    }

    #[test]
    fn vs_code_process_types() {
        let main = classify("Code.exe", CODE, &args(&[CODE]));
        assert_eq!(main.role, "VS Code: Main process");
        let host = classify(
            "Code.exe",
            CODE,
            &args(&[CODE, "--type=utility", "--utility-sub-type=node.mojom.NodeService", "--inspect-port=0"]),
        );
        assert_eq!(host.role, "VS Code: Extension host");
        let helper = classify(
            "Code.exe",
            CODE,
            &args(&[CODE, "--type=utility", "--utility-sub-type=node.mojom.NodeService"]),
        );
        assert_eq!(helper.role, "VS Code: Node helper");
        let renderer = classify("Code.exe", CODE, &args(&[CODE, "--type=renderer"]));
        assert_eq!(renderer.role, "VS Code: Window or webview");
    }

    #[test]
    fn opening_a_folder_is_not_mistaken_for_a_script() {
        let c = classify("Code.exe", CODE, &args(&[CODE, r"C:\Projects\my-app"]));
        assert_eq!(c.role, "VS Code: Main process");
    }

    #[test]
    fn claude_code_binary_in_extension_folder() {
        let exe = r"c:\Users\me\.vscode\extensions\anthropic.claude-code-2.1.314-win32-x64\resources\native-binary\claude.exe";
        let c = classify("claude.exe", exe, &args(&[exe, "--output-format", "stream-json"]));
        assert_eq!(c.category, Category::AiAgent);
        assert_eq!(c.role, "Claude Code session (editor)");
    }

    #[test]
    fn node_dev_servers() {
        let vite = classify("node.exe", r"C:\nvm4w\nodejs\node.exe", &args(&["node", r"C:\p\app\node_modules\vite\bin\vite.js"]));
        assert_eq!((vite.category, vite.role.as_str()), (Category::DevServer, "Vite dev server"));
        let build = classify("node.exe", "", &args(&["node", r"C:\p\app\node_modules\vite\bin\vite.js", "build"]));
        assert_eq!(build.category, Category::BuildTool);
        let next = classify("node.exe", "", &args(&["node", r"C:\p\app\node_modules\next\dist\bin\next", "dev"]));
        assert_eq!(next.tech.as_deref(), Some("Next.js"));
    }

    #[test]
    fn node_flags_with_values_are_skipped() {
        let c = classify("node.exe", "", &args(&["node", "--require", "dotenv/config", r"C:\p\server.js"]));
        assert_eq!(c.role, "Node.js script: server.js");
    }

    #[test]
    fn package_managers_and_mcp() {
        let npm = classify("node.exe", "", &args(&["node", r"C:\nvm4w\nodejs\node_modules\npm\bin\npm-cli.js", "run", "dev"]));
        assert_eq!(npm.category, Category::PackageManager);
        let mcp = classify("node.exe", "", &args(&["node", r"C:\x\node_modules\@modelcontextprotocol\server-filesystem\dist\index.js"]));
        assert_eq!(mcp.category, Category::McpServer);
    }

    #[test]
    fn browsers_and_webview() {
        let tab = classify("brave.exe", "", &args(&["brave.exe", "--type=renderer"]));
        assert_eq!(tab.role, "Brave: Tab or extension");
        assert_eq!(tab.category, Category::Browser);
        let main = classify("msedge.exe", "", &args(&["msedge.exe"]));
        assert_eq!(main.role, "Edge: Main process");
    }

    #[test]
    fn system_and_unknown() {
        assert_eq!(classify("svchost.exe", r"C:\Windows\System32\svchost.exe", &[]).category, Category::System);
        assert_eq!(classify("csrss.exe", "", &[]).category, Category::System);
        assert_eq!(classify("mystery.exe", "", &args(&["mystery.exe"])).category, Category::Other);
        assert_eq!(classify("Melvor2.exe", r"D:\Games\Melvor2.exe", &[]).category, Category::App);
    }

    #[test]
    fn vendor_drivers_under_windows_are_not_windows_components() {
        let nvidia = r"C:\Windows\System32\DriverStore\FileRepository\nv\NVDisplay.Container.exe";
        assert_eq!(classify("NVDisplay.Container.exe", nvidia, &[]).category, Category::App);
        assert_eq!(classify("dasHost.exe", r"C:\Windows\System32\dasHost.exe", &[]).category, Category::System);
    }

    #[test]
    fn linux_editors_and_servers_are_recognised() {
        let code = "/usr/share/code/code";
        assert_eq!(classify("code", code, &args(&[code])).role, "VS Code: Main process");
        let host = classify("code", code, &args(&[code, "--type=utility", "--utility-sub-type=node.mojom.NodeService", "--inspect-port=0"]));
        assert_eq!(host.role, "VS Code: Extension host");
        let pylance = classify("code", code, &args(&[code, "/home/me/.vscode/extensions/ms-python.vscode-pylance-2026.4.1/dist/server.bundle.js"]));
        assert_eq!(pylance.role, "Pylance language server");
        let vite = classify("node", "/usr/bin/node", &args(&["node", "/home/me/app/node_modules/vite/bin/vite.js"]));
        assert_eq!(vite.category, Category::DevServer);
    }

    #[test]
    fn rewritten_node_titles_are_recognised() {
        assert_eq!(classify("next-server (v15", "/usr/bin/node", &args(&["next-server (v15.1.0)"])).tech.as_deref(), Some("Next.js"));
        assert_eq!(classify("npm run dev", "/usr/bin/node", &args(&["npm run dev"])).category, Category::PackageManager);
    }

    #[test]
    fn firefox_children_are_named_from_their_exe() {
        let exe = "/usr/lib/firefox/firefox";
        let tab = classify("Isolated Web Co", exe, &args(&[exe, "-contentproc", "-isForBrowser", "tab"]));
        assert_eq!((tab.category, tab.role.as_str()), (Category::Browser, "Firefox: Tab or extension"));
        assert_eq!(classify("firefox", exe, &args(&[exe])).role, "Firefox: Main process");
    }

    #[test]
    fn linux_system_processes_are_recognised() {
        for (name, exe) in [
            ("systemd-journald", "/usr/lib/systemd/systemd-journald"),
            ("gnome-shell", "/usr/bin/gnome-shell"),
            ("gsd-color", "/usr/libexec/gsd-color"),
            ("NetworkManager", "/usr/sbin/NetworkManager"),
            ("sshd", "/usr/sbin/sshd"),
        ] {
            assert_eq!(classify(name, exe, &[]).category, Category::System, "{name}");
        }
        assert_eq!(classify("nginx", "/usr/sbin/nginx", &[]).category, Category::DevServer);
        assert_eq!(classify("bash", "/usr/bin/bash", &[]).category, Category::Terminal);
    }

    #[test]
    fn multiplexers_are_not_plain_shells() {
        assert!(is_shell_role(&classify("zsh", "/usr/bin/zsh", &[]).role));
        assert!(!is_shell_role(&classify("tmux", "/usr/bin/tmux", &[]).role));
    }

    #[test]
    fn git_bash_utilities_are_shell_commands() {
        let c = classify("sleep.exe", r"C:\Program Files\Git\usr\bin\sleep.exe", &[]);
        assert_eq!((c.category, c.role.as_str()), (Category::Terminal, "Shell command: sleep"));
    }

    #[test]
    fn databases_and_python_servers() {
        assert_eq!(classify("postgres.exe", r"C:\pg\postgres.exe", &[]).category, Category::Database);
        let dj = classify("python.exe", "", &args(&["python", "manage.py", "runserver"]));
        assert_eq!(dj.role, "Django dev server");
        let http = classify("python.exe", "", &args(&["python", "-m", "http.server", "8799"]));
        assert_eq!((http.category, http.role.as_str()), (Category::DevServer, "Python HTTP server"));
        let module = classify("python.exe", "", &args(&["python", "-m", "pip", "install"]));
        assert_eq!(module.role, "Python module: pip");
    }

    #[test]
    fn extension_id_trims_versions() {
        assert_eq!(
            extension_id("c:/users/me/.vscode/extensions/ms-python.vscode-pylance-2026.4.1/dist/x.js").as_deref(),
            Some("ms-python.vscode-pylance")
        );
        assert_eq!(
            extension_id("c:/users/me/.vscode/extensions/anthropic.claude-code-2.1.314-win32-x64/r/claude.exe").as_deref(),
            Some("anthropic.claude-code")
        );
        assert_eq!(extension_id("c:/other/path.js"), None);
    }
}
