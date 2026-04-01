//! Configuration type definitions.

use std::path::PathBuf;

use ammonia::Url;
use wdl_analysis::Config as AnalysisConfig;

/// External URLs related to the project.
#[derive(Clone, Debug, Default)]
pub struct ExternalUrls {
    /// URL pointing to the project's homepage.
    pub homepage: Option<Url>,
    /// URL pointing to the project's GitHub repository.
    pub github: Option<Url>,
}

/// The location to embed an arbitrary JavaScript `<script>` tag into each HTML
/// page.
#[derive(Debug, Default)]
pub struct AdditionalHtml {
    /// Embed the contents immediately before the closing `</head>` tag.
    head: Option<String>,
    /// Embed the contents immediately after the opening `<body>` tag.
    body_open: Option<String>,
    /// Embed the contents immediately before the closing `</body>` tag.
    body_close: Option<String>,
}

impl AdditionalHtml {
    /// Create a new [`AdditionalHtml`] struct.
    pub fn new(
        head: Option<String>,
        body_open: Option<String>,
        body_close: Option<String>,
    ) -> Self {
        Self {
            head,
            body_open,
            body_close,
        }
    }

    /// Get the HTML to add to the head.
    pub fn head(&self) -> Option<&str> {
        self.head.as_deref()
    }

    /// Get the HTML to add to the body open.
    pub fn body_open(&self) -> Option<&str> {
        self.body_open.as_deref()
    }

    /// Get the HTML to add to the body close.
    pub fn body_close(&self) -> Option<&str> {
        self.body_close.as_deref()
    }
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
    /// Optional HTML to embed in each page.
    pub(crate) additional_html: AdditionalHtml,
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
            additional_html: AdditionalHtml::default(),
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

    /// Overwrite the config's additional HTML with the new value.
    pub fn additional_html(mut self, additional_html: AdditionalHtml) -> Self {
        self.additional_html = additional_html;
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
