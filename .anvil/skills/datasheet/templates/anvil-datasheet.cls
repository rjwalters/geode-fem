%% anvil-datasheet.cls — LaTeX class for anvil:datasheet
%%
%% Class for customer-facing IC / component datasheets — the spec-bearing
%% document a customer designs against. Extracted from the studio canary's two
%% hand-authored preliminary datasheets (a dual-SKU edge-AI part family sharing
%% one base die — issue #418). It shares the same preamble family as
%% anvil-proposal.cls; the substantive differences are the navy accent (the
%% shared figure-palette ANVIL_NAVY), the part-vendor title block + rev/footer
%% conventions, the two-column first-page support (multicol), and the
%% measured-vs-projected provenance macros.
%%
%% ── Engine ──────────────────────────────────────────────────────────────────
%% Compile with XeLaTeX (NOT pdflatex): the class uses fontspec for system fonts.
%%   xelatex datasheet.tex
%%
%% ── Fonts ───────────────────────────────────────────────────────────────────
%% Default main font is Helvetica Neue with an automatic fallback to Latin
%% Modern Sans via \IfFontExistsTF, so the class compiles on a stock TeX Live
%% install with no system fonts.
%%
%% ── Provides ────────────────────────────────────────────────────────────────
%%   \datasheetpart, \datasheettitle, \datasheetsubtitle, \datasheetcompany,
%%   \datasheetdate, \datasheetrev, \datasheetstatus
%%                       — title-block + footer metadata setters
%%   \maketitleblock     — renders the part-vendor title block + accent rule
%%   featurecolumns (env)— two-column first-page block (Key Features | Applications)
%%   \est{v}             — estimated value: renders "v (est.)" in muted type
%%   \simval{v}          — simulated value: renders "v (sim.)" in muted type
%%   \meas{v}            — silicon-measured value: renders bare (semantic marker)
%%   \preliminarynotice  — standing "specifications subject to change" notice
%%   callout (tcolorbox) — left accent border; used for notes / warnings
%%   specbox (tcolorbox) — borderless bg fill; used for spec tables
%%   colors: ink, accent, muted, bg, rule
%%
%% ── Footer convention ───────────────────────────────────────────────────────
%% Every page footer carries: <company> --- <part> Rev <rev> --- <date> on the
%% left and page X / Y on the right — the consistent rev/footer block the
%% canary's layout-conventions recommendation calls for.

\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{anvil-datasheet}[2026/06/11 v0.1 Anvil IC/component datasheet class]

\DeclareOption*{\PassOptionsToClass{\CurrentOption}{article}}
\ProcessOptions\relax

%% Base class — single-column article, 10pt, US Letter
\LoadClass[10pt,letterpaper]{article}

%% ─── Typography ──────────────────────────────────────────────────────────────
\RequirePackage{fontspec}
\IfFontExistsTF{Helvetica Neue}{%
  \setmainfont{Helvetica Neue}[
    UprightFont    = *,
    BoldFont       = * Bold,
    ItalicFont     = * Italic,
    BoldItalicFont = * Bold Italic,
  ]%
}{%
  \setmainfont{Latin Modern Sans}%
}
\IfFontExistsTF{Menlo}{%
  \setmonofont{Menlo}[Scale=0.85]%
}{%
  \setmonofont{Latin Modern Mono}[Scale=0.85]%
}

%% ─── Layout ──────────────────────────────────────────────────────────────────
\RequirePackage[
  letterpaper,
  top=0.7in,
  bottom=0.9in,
  left=0.85in,
  right=0.85in,
]{geometry}
\RequirePackage{parskip}
\setlength{\parskip}{0.4em}
\RequirePackage{enumitem}
\setlist{nosep, leftmargin=1.5em, itemsep=2pt}
\RequirePackage{multicol}
\setlength{\columnsep}{2em}

%% ─── Colors ──────────────────────────────────────────────────────────────────
\RequirePackage{xcolor}
\definecolor{ink}{HTML}{1A1A1F}        % near-black for body
\definecolor{accent}{HTML}{1F4E7A}     % navy — ANVIL_NAVY from the shared palette
\definecolor{muted}{HTML}{6B7280}      % gray for secondary text + provenance labels
\definecolor{bg}{HTML}{F5F7FA}         % cool light fill for callouts / spec boxes
\definecolor{rule}{HTML}{1F4E7A}

\color{ink}

%% ─── Headings ────────────────────────────────────────────────────────────────
\RequirePackage{titlesec}
\RequirePackage{needspace}

\titleformat{\section}
  {\needspace{6\baselineskip}\large\bfseries\color{accent}}
  {}
  {0em}
  {}
  [\vspace{-0.4em}{\color{rule}\rule{\textwidth}{1.0pt}}\vspace{0.1em}]

\titleformat{\subsection}
  {\normalsize\bfseries\color{ink}}
  {}
  {0em}
  {}

\titlespacing*{\section}{0pt}{1.0em}{0.3em}
\titlespacing*{\subsection}{0pt}{0.6em}{0.15em}

%% ─── Tables ──────────────────────────────────────────────────────────────────
\RequirePackage{booktabs}
\RequirePackage{tabularx}
\RequirePackage{array}
\renewcommand{\arraystretch}{1.3}

%% ─── Callout + specbox ───────────────────────────────────────────────────────
\RequirePackage{tcolorbox}
\tcbuselibrary{skins, breakable}

%% callout — left accent border; used for notes / warnings / the preliminary notice.
\newtcolorbox{callout}[1][]{%
  enhanced,
  breakable,
  colback=bg,
  colframe=bg,
  boxrule=0pt,
  borderline west={3pt}{0pt}{accent},
  sharp corners,
  left=12pt, right=12pt, top=8pt, bottom=8pt,
  fonttitle=\bfseries\color{accent},
  coltitle=accent,
  attach boxed title to top left={yshift=-2mm, xshift=4mm},
  boxed title style={colback=bg, colframe=bg, boxrule=0pt, sharp corners},
  #1
}

%% specbox — borderless bg fill; used for spec / characteristics / ordering tables.
\newtcolorbox{specbox}{%
  enhanced,
  breakable,
  colback=bg,
  colframe=bg,
  boxrule=0pt,
  sharp corners,
  left=12pt, right=12pt, top=6pt, bottom=6pt,
}

%% ─── Header / Footer ─────────────────────────────────────────────────────────
\RequirePackage{fancyhdr}
\RequirePackage{lastpage}

%% Metadata setters (also feed the footer).
%% etoolbox supplies \ifdefempty for the empty-subtitle guard in the title
%% block. Already loaded transitively by tcolorbox; the explicit load is
%% idempotent and self-documenting. The expansion-based \ifdefempty replaces
%% the legacy \ifx-against-\empty idiom, which silently depends on
%% prefix equality: \ifx is false whenever the operands differ in
%% \long/\protected status, so it breaks if the initialization ever becomes
%% \long (e.g. older kernels' \newcommand, or a future \NewDocumentCommand
%% refactor). See issue #422.
\RequirePackage{etoolbox}
\newcommand{\anvil@part}{PART-0000}
\newcommand{\anvil@title}{Datasheet Title}
\newcommand{\anvil@subtitle}{}
\newcommand{\anvil@company}{Company}
\newcommand{\anvil@date}{\today}
\newcommand{\anvil@rev}{0.1}
\newcommand{\anvil@status}{PRELIMINARY --- SPECIFICATIONS SUBJECT TO CHANGE}
\newcommand{\datasheetpart}[1]{\renewcommand{\anvil@part}{#1}}
\newcommand{\datasheettitle}[1]{\renewcommand{\anvil@title}{#1}}
\newcommand{\datasheetsubtitle}[1]{\renewcommand{\anvil@subtitle}{#1}}
\newcommand{\datasheetcompany}[1]{\renewcommand{\anvil@company}{#1}}
\newcommand{\datasheetdate}[1]{\renewcommand{\anvil@date}{#1}}
\newcommand{\datasheetrev}[1]{\renewcommand{\anvil@rev}{#1}}
\newcommand{\datasheetstatus}[1]{\renewcommand{\anvil@status}{#1}}

%% Consistent rev/footer block on every page (canary layout convention 3).
\pagestyle{fancy}
\fancyhf{}
\renewcommand{\headrulewidth}{0pt}
\fancyfoot[L]{\footnotesize\color{muted}\anvil@company\ --- \anvil@part\ Rev \anvil@rev\ --- \anvil@date}
\fancyfoot[R]{\footnotesize\color{muted}\thepage\ / \pageref{LastPage}}

%% ─── Hyperlinks ──────────────────────────────────────────────────────────────
\RequirePackage[hidelinks]{hyperref}
\hypersetup{colorlinks=true, linkcolor=accent, urlcolor=accent}

%% ─── Graphics ────────────────────────────────────────────────────────────────
\RequirePackage{graphicx}
\RequirePackage{microtype}

%% ─── Title block ─────────────────────────────────────────────────────────────
%% \maketitleblock — renders the company kicker, part number, title, subtitle,
%% accent rule, and the date / rev / status line. Part-vendor convention: the
%% part number is the headline; the title is the descriptive line under it.
\newcommand{\maketitleblock}{%
  \thispagestyle{fancy}%
  \begin{flushleft}%
    {\color{muted}\footnotesize\textsf{\MakeUppercase{\anvil@company}}}\\[0.8em]%
    {\fontsize{24}{28}\selectfont\bfseries\color{ink}\anvil@part}\\[0.25em]%
    {\large\bfseries\color{accent}\anvil@title}\\[0.2em]%
    \ifdefempty{\anvil@subtitle}{}{%
      {\normalsize\color{muted}\itshape\anvil@subtitle}\\[0.5em]%
    }%
    {\color{rule}\rule{\textwidth}{2pt}}\\[0.2em]%
    {\footnotesize\color{muted}\anvil@date\ \textbar\ Rev \anvil@rev \hfill \anvil@status}%
  \end{flushleft}%
  \vspace{0.4em}%
}

%% ─── Two-column first-page block ─────────────────────────────────────────────
%% featurecolumns — the Key Features | Applications side-by-side layout a
%% customer expects on a datasheet's first page. Body supplies the two columns
%% separated by \columnbreak.
\newenvironment{featurecolumns}{%
  \begin{multicols}{2}\raggedcolumns
}{%
  \end{multicols}%
}

%% ─── Measured-vs-projected provenance macros ─────────────────────────────────
%% \est{v}    — estimated (pre-silicon) value: "v (est.)" with a muted label.
%% \simval{v} — value from system-model / RTL simulation: "v (sim.)".
%% \meas{v}   — silicon-measured value: bare. Exists as a semantic marker so a
%%              future mechanical provenance check can distinguish "deliberately
%%              bare" from "unlabeled".
\newcommand{\est}[1]{#1~{\footnotesize\color{muted}(est.)}}
\newcommand{\simval}[1]{#1~{\footnotesize\color{muted}(sim.)}}
\newcommand{\meas}[1]{#1}

%% \preliminarynotice — the standing notice a `status: preliminary` sheet must
%% carry (rubric dim 8). Drop into the legal/notices section.
\newcommand{\preliminarynotice}{%
  \begin{callout}[title=Preliminary]
  This document contains preliminary information on a product in development.
  Specifications are based on design models and simulation unless explicitly
  marked as silicon-measured, and are subject to change without notice.
  Characterization is pending.
  \end{callout}%
}

\endinput
