#!/usr/bin/env fish
set -e

set -g GITHUB_USER "what386"
set -g GITHUB_REPO "upstream-rs"
set -g BINARY_NAME "upstream-rs"
set -g TMP_DIR ""

function cleanup
    if test -n "$TMP_DIR"; and test -d "$TMP_DIR"
        rm -rf "$TMP_DIR"
    end
end

function detect_arch
    switch (uname -m)
        case x86_64 amd64
            echo "x86_64"
        case aarch64 arm64
            echo "aarch64"
        case armv7l
            echo "armv7"
        case i386 i686
            echo "i686"
        case '*'
            echo "unknown"
    end
end

function detect_os
    switch (uname -s)
        case Linux
            echo "unknown-linux-gnu"
        case Darwin
            echo "apple-darwin"
        case '*'
            echo "unknown"
    end
end

function download_file --argument-names url out
    if type -q curl
        curl -fsSL "$url" -o "$out"
        return $status
    else if type -q wget
        wget -q "$url" -O "$out"
        return $status
    end

    return 127
end

function install_completion
    set completion_url "https://github.com/$GITHUB_USER/$GITHUB_REPO/releases/latest/download/completions.fish"
    set completion_tmp "$TMP_DIR/completions.fish"
    set dest_dir "$HOME/.config/fish/completions"
    set dest_file "$dest_dir/upstream.fish"

    echo "Installing fish completion..."

    if not download_file "$completion_url" "$completion_tmp"
        echo "Warning: Failed to download fish completion from $completion_url"
        return 0
    end

    if not mkdir -p "$dest_dir"
        echo "Warning: Failed to create completion directory $dest_dir"
        return 0
    end

    if not cp "$completion_tmp" "$dest_file"
        echo "Warning: Failed to install fish completion to $dest_file"
        return 0
    end

    echo "Fish completion installed to $dest_file"
end

function main
    echo "Starting installation..."

    set arch (detect_arch)
    set os (detect_os)

    if test "$arch" = "unknown"
        echo "Error: Unsupported architecture ($arch)"
        exit 1
    end

    if test "$os" = "unknown"
        echo "Error: Unsupported operating system"
        exit 1
    end

    echo "Detected OS: $os"
    echo "Detected Architecture: $arch"

    set download_url "https://github.com/$GITHUB_USER/$GITHUB_REPO/releases/latest/download/$BINARY_NAME-$arch-$os"
    echo "Downloading from: $download_url"

    set -g TMP_DIR (mktemp -d)
    set tmp_file "$TMP_DIR/$BINARY_NAME"

    if not download_file "$download_url" "$tmp_file"
        echo "Error: Failed to download upstream binary."
        cleanup
        exit 1
    end

    chmod +x "$tmp_file"
    echo "Download complete!"

    echo "Running command 1/2: init"
    if not "$tmp_file" init
        echo "Error: Command failed: init"
        cleanup
        exit 1
    end

    echo "Running command 2/2: install upstream what386/upstream-rs -k binary"
    if not "$tmp_file" install upstream what386/upstream-rs -k binary
        echo "Error: Command failed: install upstream what386/upstream-rs -k binary"
        cleanup
        exit 1
    end

    # Best-effort completion setup; do not fail installation if it cannot be configured.
    install_completion

    cleanup
    echo "Installation complete!"
end

main "$argv"
