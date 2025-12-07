use chrono::{DateTime, Utc};
use std::path::Path;

use crate::models::common::Version;
use crate::models::provider::{Asset, Release};
use crate::infrastructure::providers::{GithubAdapter};

#[derive(Debug, Clone)]
pub struct AdapterManager{
    github_adapter: GithubClient,
}

impl AdapterManager {
    pub fn new(github_adapter: GithubAdapter,
               github_credentials: String) {
        let github_client: GithubClient = github_client.new();
        let github_adapter = github_adapter.new(github_client);
        Self {
            github_adapter: github_adapter
        }
    }
}

public class ProviderManager
{
    private static readonly Architecture _architecture = RuntimeInformation.OSArchitecture;
    private static readonly OSInfo.OSKind _operatingSystem = OSInfo.OS;

    private readonly GithubClient _githubClient;
    private readonly GithubAdapter _githubAdapter;

    private readonly string _downloadCachePath;

    public record PlatformInfo(
        OSPlatform Platform,
        Architecture Arch
    );

    public record Credentials
    {
        public string? GithubToken;

        public Credentials(string? githubToken)
        {
            GithubToken = githubToken;
        }
    }

    public ProviderManager(Credentials credentials)
    {
        _githubClient = new GithubClient(credentials.GithubToken);
        _githubAdapter = new GithubAdapter(_githubClient);

        _downloadCachePath = Path.Combine(
            Path.GetTempPath(),
            "upstream_downloads"
        );
        Directory.CreateDirectory(_downloadCachePath);
    }

    /// <summary>
    /// Gets the latest release for a package from its configured provider.
    /// </summary>
    public async Task<Release> GetLatestPackageRelease(Package package)
    {
        return package.Provider switch
        {
            Provider.Github => await _githubAdapter.GetLatestRelease(package.Slug),
            _ => throw new NotSupportedException($"Provider {package.Provider} is not supported")
        };
    }

    /// <summary>
    /// Gets the latest release for a repository from a specific provider.
    /// </summary>
    public async Task<Release> GetLatestRelease(string slug, Provider provider)
    {
        return provider switch
        {
            Provider.Github => await _githubAdapter.GetLatestRelease(slug),
            _ => throw new NotSupportedException($"Provider {provider} is not supported")
        };
    }

    /// <summary>
    /// Gets all releases for a package from its configured provider.
    /// </summary>
    public async Task<Release[]> GetAllReleases(Package package, int perPage = 30)
    {
        return package.Provider switch
        {
            Provider.Github => await _githubAdapter.GetAllReleases(package.Slug, perPage),
            _ => throw new NotSupportedException($"Provider {package.Provider} is not supported")
        };
    }

    /// <summary>
    /// Gets a specific release by tag from a package's provider.
    /// </summary>
    public async Task<Release> GetReleaseByTag(Package package, string tag)
    {
        return package.Provider switch
        {
            Provider.Github => await _githubAdapter.GetReleaseByTag(package.Slug, tag),
            _ => throw new NotSupportedException($"Provider {package.Provider} is not supported")
        };
    }

    /// <summary>
    /// Checks if an update is available for the package based on its update channel.
    /// </summary>
    public async Task<bool> IsUpdateAvailable(Package package)
    {
        if (package.IsPaused)
            return false;

        try
        {
            var latest = await GetLatestPackageRelease(package);

            // Filter based on update channel
            if (!ShouldConsiderRelease(latest, package.Channel))
                return false;

            return latest.Version.IsNewerThan(package.Version);
        }
        catch
        {
            return false;
        }
    }

    /// <summary>
    /// Gets the recommended asset for a package from a release.
    /// Prioritizes by AssetPattern, then by Filetype.
    /// "Auto" defaults to appimages -> binaries -> archives.
    /// </summary>
    public bool TryGetRecommendedAsset(Release release, Package package, out Asset? asset, out string? message)
    {
        message = null;

        // Priority 1: Use AssetPattern if specified
        if (!string.IsNullOrWhiteSpace(package.AssetPattern))
        {
            var patternMatch = release.GetAssetByPattern(package.AssetPattern);
            if (patternMatch != null && IsCompatibleArchitecture(patternMatch))
            {
                asset = patternMatch;

                if (_architecture == Architecture.X64 && asset.TargetArch == Architecture.X86)
                    message = "Fallback to 32-bit (x86) binary on 64-bit system";

                if (_architecture == Architecture.Arm64 && asset.TargetArch == Architecture.Arm)
                    message = "Fallback to 32-bit (ARM) binary on 64-bit system";

                return true;
            }
        }

        Asset? kindMatch = null;

        // Priority 2: Use specific Filetype
        kindMatch = GetAssetByKindFiltered(release, package);

        if (kindMatch != null)
        {
            asset = kindMatch;

            if (_architecture == Architecture.X64 && asset.TargetArch == Architecture.X86)
                message = "Fallback to 32-bit (x86) binary on 64-bit system";

            if (_architecture == Architecture.Arm64 && asset.TargetArch == Architecture.Arm)
                message = "Fallback to 32-bit (ARM) binary on 64-bit system";

            return true;
        }

        asset = null;
        message = "Failed to find compatible asset";
        return false;
    }

    /// <summary>
    /// Downloads an asset for a package to a local path.
    /// </summary>
    public async Task<string> DownloadAsset(
        Asset asset,
        Provider provider,
        IProgress<(long downloadedBytes, long totalBytes)>? downloadProgress = null)
    {
        string fileName = Path.GetFileName(asset.Name);
        string downloadPath = Path.Combine(_downloadCachePath, fileName);

        switch (provider)
        {
            case Provider.Github:
                await _githubAdapter.DownloadAsset(asset, downloadPath, downloadProgress);
                break;
        }

        return downloadPath;
    }

    public Asset? GetAssetByKindFiltered(Release release, Package package)
    {
        var assets = release.Assets.Where(a => a.Filetype == package.Filetype).ToList();
        if (!assets.Any())
            return null;

        var compatibleAssets = assets
            .Where(a => IsCompatibleArchitecture(a) && IsCompatibleOS(a))
            .ToList();

        if (!compatibleAssets.Any())
            return null;

        // Score each asset and return the best one
        return compatibleAssets
            .Select(a => new { Asset = a, Score = CalculateAssetScore(a, package) })
            .OrderByDescending(x => x.Score)
            .First()
            .Asset;
    }

    private int CalculateAssetScore(Asset asset, Package package)
    {
        string name = asset.Name.ToLowerInvariant();
        int score = 0;

        if (asset.TargetArch.Equals(_architecture))
            score += 80; // Architecture match bonus

        if (asset.TargetOS.Equals(_operatingSystem))
            score += 60; // OS match bonus

        if (!name.Contains(package.Name.ToLowerInvariant()))
            score -= 40; // Name mismatch penalty- could be metadata

        if (asset.Size < 100000) // 100kb
            score -= 20; // Very small files are suspicious

        if (asset.Filetype == Filetype.Archive)
        {
            if (name.EndsWith(".tar.gz") || name.EndsWith(".tgz"))
                score += 10; // preserves file permissions
            else if (name.EndsWith(".zip"))
                score += 5;
        }

        if (asset.Filetype == Filetype.Compressed)
        {
            if (name.EndsWith(".br"))
                score += 10; // often smaller than gzip
            else if (name.EndsWith(".gz"))
                score += 5;
        }

        return score;
    }

    private bool IsCompatibleOS(Asset asset)
    {
        if (!asset.TargetOS.HasValue)
            return true; // unknown -> assume possible

        return asset.TargetOS.Value.Equals(_operatingSystem);
    }

    private bool IsCompatibleArchitecture(Asset asset)
    {
        if (!asset.TargetArch.HasValue)
            return true; // unknown -> assume possible

        var target = asset.TargetArch.Value;
        var host = _architecture;

        // x64 can run x86
        if (host == Architecture.X64 && target == Architecture.X86)
            return true;

        // arm64 can run arm32
        if (host == Architecture.Arm64 && target == Architecture.Arm)
            return true;

        // otherwise, different arch is bad
        return target == host;
    }

    private static bool ShouldConsiderRelease(Release release, Channel channel)
    {
        return channel switch
        {
            Channel.Stable => !release.IsDraft && !release.IsPrerelease,
            Channel.Beta => !release.IsDraft,
            Channel.Nightly => !release.IsDraft,
            Channel.All => true,
            _ => false
        };
    }
