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
    set helper_url "https://raw.githubusercontent.com/$GITHUB_USER/$GITHUB_REPO/main/scripts/install/completions.sh"
    set helper_script "$TMP_DIR/completions.sh"

    echo "Installing fish completion..."

    if not download_file "$helper_url" "$helper_script"
        echo "Warning: Failed to download completion installer from $helper_url"
        return 0
    end

    if not chmod +x "$helper_script"
        echo "Warning: Failed to make completion installer executable"
        return 0
    end

    if not "$helper_script" fish
        echo "Warning: Completion installer failed for fish"
        return 0
    end

    echo "Fish completion installed"
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

    echo "Running command 1/2: hooks init"
    if not "$tmp_file" hooks init
        echo "Error: Command failed: hooks init"
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
