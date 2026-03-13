//! Configuration type definitions.

use std::path::PathBuf;

use ammonia::Url;
use wdl_analysis::Config as AnalysisConfig;

use crate::PREFER_FULL_DIRECTORY;

/// External URLs related to the project.
#[derive(Clone, Debug, Default)]
pub struct ExternalUrls {
    /// URL pointing to the project's homepage.
    pub homepage: Option<Url>,
    /// URL pointing to the project's GitHub repository.
    pub github: Option<Url>,
}

/// The location to embed an arbitrary JaveScript `<script>` tag into each HTML
/// page.
#[derive(Debug)]
pub enum AdditionalScript {
    /// Embed the contents immediately after the opening `<head>` tag.
    HeadOpen(String),
    /// Embed the contents immediately before the closing `</head>` tag.
    HeadClose(String),
    /// Embed the contents immediately after the opening `<body>` tag.
    BodyOpen(String),
    /// Embed the contents immediately before the closing `</body>` tag.
    BodyClose(String),
    /// Don't embed any script.
    None,
}

/// Configuration for documentation generation.
#[derive(Debug)]
pub struct Config {
    /// Configuration to use for analysis.
    pub(crate) analysis_config: AnalysisConfig,
    /// WDL workspace that should be documented.
    pub(crate) workspace: PathBuf,
    /// Output location for the documentation.
    pub(crate) output_dir: PathBuf,
    /// An optional markdown file to embed in the homepage.
    pub(crate) homepage: Option<PathBuf>,
    /// Initialize pages in light mode instead of the default dark mode.
    pub(crate) init_light_mode: bool,
    /// An optional custom theme directory.
    pub(crate) custom_theme: Option<PathBuf>,
    /// An optional custom logo to embed in the left sidebar.
    pub(crate) custom_logo: Option<PathBuf>,
    /// External URLs related to the project.
    pub(crate) external_urls: ExternalUrls,
    /// An optional alternate (light mode) custom logo to embed in the left
    /// sidebar.
    pub(crate) alt_logo: Option<PathBuf>,
    /// Optional JavaScript to embed in each HTML page.
    pub(crate) additional_javascript: AdditionalScript,
    /// Initialize pages on the "Full Directory" view instead of the "Workflows"
    /// view of the left sidebar.
    pub(crate) init_on_full_directory: bool,
    /// (**EXPERIMENTAL**) Enable support for documentation comments.
    pub(crate) enable_doc_comments: bool,
}

impl Config {
    /// Create a new documentation configuration.
    pub fn new(
        analysis_config: AnalysisConfig,
        workspace: impl Into<PathBuf>,
        output_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            analysis_config,
            workspace: workspace.into(),
            output_dir: output_dir.into(),
            homepage: None,
            init_light_mode: false,
            custom_theme: None,
            custom_logo: None,
            external_urls: ExternalUrls::default(),
            alt_logo: None,
            additional_javascript: AdditionalScript::None,
            init_on_full_directory: PREFER_FULL_DIRECTORY,
            enable_doc_comments: false,
        }
    }

    /// Overwrite the config's homepage with the new value.
    pub fn homepage(mut self, homepage: Option<PathBuf>) -> Self {
        self.homepage = homepage;
        self
    }

    /// Overwrite the config's light mode default with the new value.
    pub fn init_light_mode(mut self, init_light_mode: bool) -> Self {
        self.init_light_mode = init_light_mode;
        self
    }

    /// Overwrite the config's custom theme with the new value.
    pub fn custom_theme(mut self, custom_theme: Option<PathBuf>) -> Self {
        self.custom_theme = custom_theme;
        self
    }

    /// Overwrite the config's custom logo with the new value.
    pub fn custom_logo(mut self, custom_logo: Option<PathBuf>) -> Self {
        self.custom_logo = custom_logo;
        self
    }

    /// Overwrite the config's external URLs with the new value.
    pub fn external_urls(mut self, external_urls: ExternalUrls) -> Self {
        self.external_urls = external_urls;
        self
    }

    /// Overwrite the config's alternate logo with the new value.
    pub fn alt_logo(mut self, alt_logo: Option<PathBuf>) -> Self {
        self.alt_logo = alt_logo;
        self
    }

    /// Overwrite the config's additional JS with the new value.
    pub fn additional_javascript(mut self, additional_javascript: AdditionalScript) -> Self {
        self.additional_javascript = additional_javascript;
        self
    }

    /// Overwrite the config's init_on_full_directory with the new value.
    pub fn prefer_full_directory(mut self, prefer_full_directory: bool) -> Self {
        self.init_on_full_directory = prefer_full_directory;
        self
    }

    /// Enable support for documentation comments.
    ///
    /// NOTE: This is an experimental option, and will be removed in a future
    /// major release.
    ///
    /// For more information, see the pre-RFC discussion
    /// [here](https://github.com/openwdl/wdl/issues/757).
    pub fn enable_doc_comments(mut self, enable_doc_comments: bool) -> Self {
        self.enable_doc_comments = enable_doc_comments;
        self
    }
}
