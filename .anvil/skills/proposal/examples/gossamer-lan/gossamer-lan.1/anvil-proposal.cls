%% anvil-proposal.cls — LaTeX class for anvil:proposal
%%
%% Class for buildable-system concept proposals — the pre-commitment document
%% that pitches a concrete buildable system to whoever holds the commitment (an
%% external client, or an internal budget sponsor). Extracted from the canonical
%% "Gossamer LAN" worked instance (a hair-thin single-mode fiber network threaded
%% invisibly through a palazzo), the grounding example for this skill. It shares
%% the same preamble family as anvil-installation.cls — the only substantive
%% difference is the signature accent palette (steel blue vs. amber).
%%
%% ── Engine ──────────────────────────────────────────────────────────────────
%% Compile with XeLaTeX (NOT pdflatex): the class uses fontspec for system fonts.
%%   xelatex proposal.tex
%% This differs from anvil:paper (pdflatex). The proposal-figures command's render
%% guidance notes the engine.
%%
%% ── Fonts ───────────────────────────────────────────────────────────────────
%% Default main font is Helvetica Neue (the signature face of the worked
%% example). Consumers without Helvetica Neue installed get an automatic
%% fallback to Latin Modern Sans via \IfFontExistsTF, so the class compiles on a
%% stock TeX Live install with no system fonts. Override by dropping a
%% replacement .cls into .anvil/skills/proposal/templates/ in your repo, or by
%% passing a different \setmainfont before \begin{document}.
%%
%% ── Provides ────────────────────────────────────────────────────────────────
%%   \proposaltitle, \proposalsubtitle, \proposalstudio,
%%   \proposaldate, \proposalstage  — title-block metadata setters
%%   \maketitleblock  — renders the title block + accent rule
%%   \herofigure{path}  — full-width hero image (no-op if path is empty)
%%   \coda{text}  — closing accent rule + italic coda
%%   callout (tcolorbox)   — left accent border; used for the Premise
%%   metricbox (tcolorbox) — borderless bg fill; used for spec/BOM/cost tables
%%   colors: ink, accent, muted, bg, rule
%%
%% ── Options ─────────────────────────────────────────────────────────────────
%%   landscape — switches the geometry orientation from the default portrait to
%%               landscape letter (792 x 612 pt). Driven from the BRIEF.md
%%               `orientation:` frontmatter key by the proposal.tex.j2 template;
%%               useful for table-dense proposals (multi-section priced BOMs,
%%               4+ column comparison tables, multi-domain coverage matrices)
%%               where the wider text block lets columns breathe. Default
%%               portrait (matches Gossamer LAN).
%%   (other)  — the signature accent color is set via \definecolor in the preamble
%%              below; the template overrides it from the brief's signature_color
%%              when set.

\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{anvil-proposal}[2026/05/29 v0.1 Anvil buildable-system proposal class]

%% landscape orientation flag — declared option that sets a boolean the geometry
%% block conditionally honors. Default false (portrait). See \ifanvil@landscape
%% in the layout section below.
\newif\ifanvil@landscape
\anvil@landscapefalse
\DeclareOption{landscape}{\anvil@landscapetrue}
\DeclareOption*{\PassOptionsToClass{\CurrentOption}{article}}
\ProcessOptions\relax

%% Base class — single-column article, 10pt, US Letter (matches Gossamer LAN)
\LoadClass[10pt,letterpaper]{article}

%% ─── Typography ──────────────────────────────────────────────────────────────
\RequirePackage{fontspec}
%% Helvetica Neue with documented fallback to Latin Modern Sans (ships with
%% every TeX Live install) so the class compiles without system fonts.
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
%% The geometry package honors the class-level landscape option (set by the
%% template from the brief's `orientation: landscape` frontmatter key). The
%% landscape option, when set, is forwarded via \PassOptionsToPackage so the
%% geometry option list stays a plain comma-separated key=value sequence (an
%% inline \ifanvil@landscape ... \fi inside the option list expands awkwardly
%% during geometry's preprocessing). The geometry call carries `letterpaper`
%% explicitly so the paper dimensions are reset in the same pass that the
%% landscape orientation flag applies — without it, geometry's mode toggle
%% does not swap paperwidth/paperheight.
\ifanvil@landscape
  \PassOptionsToPackage{landscape}{geometry}
\fi
\RequirePackage[
  letterpaper,
  top=0.7in,
  bottom=0.8in,
  left=0.95in,
  right=0.95in,
]{geometry}
\RequirePackage{parskip}
\setlength{\parskip}{0.4em}
\RequirePackage{enumitem}
\setlist{nosep, leftmargin=1.5em, itemsep=2pt}

%% ─── Colors ──────────────────────────────────────────────────────────────────
\RequirePackage{xcolor}
\definecolor{ink}{HTML}{1A1A1F}        % near-black for body
\definecolor{accent}{HTML}{4A6FA5}     % steel blue — fiber-optic glass; the signature color
\definecolor{muted}{HTML}{6B7280}      % gray for secondary text
\definecolor{bg}{HTML}{F5F7FA}         % cool light fill for callouts
\definecolor{rule}{HTML}{4A6FA5}

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

%% ─── Callout + metricbox ─────────────────────────────────────────────────────
\RequirePackage{tcolorbox}
\tcbuselibrary{skins, breakable}

%% callout — left accent border; used for the Premise.
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

%% metricbox — borderless bg fill; used for spec/BOM/cost/throughput tables.
\newtcolorbox{metricbox}{%
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
%% Studio name for the footer; set via \proposalstudio, defaults sensibly.
\newcommand{\anvil@studio}{Studio}
\newcommand{\proposalstudio}[1]{\renewcommand{\anvil@studio}{#1}}
\pagestyle{fancy}
\fancyhf{}
\renewcommand{\headrulewidth}{0pt}
\fancyfoot[L]{\footnotesize\color{muted}\anvil@studio\ --- Concept Proposal}
\fancyfoot[R]{\footnotesize\color{muted}\thepage\ / \pageref{LastPage}}

%% ─── Hyperlinks ──────────────────────────────────────────────────────────────
\RequirePackage[hidelinks]{hyperref}
\hypersetup{colorlinks=true, linkcolor=accent, urlcolor=accent}

%% ─── Graphics ────────────────────────────────────────────────────────────────
\RequirePackage{graphicx}
\RequirePackage{microtype}

%% ─── Title-block metadata + renderer ─────────────────────────────────────────
\newcommand{\anvil@title}{Proposal Title}
\newcommand{\anvil@subtitle}{}
\newcommand{\anvil@date}{\today}
\newcommand{\anvil@stage}{NETWORKING DESIGN --- CONCEPT STAGE}
\newcommand{\proposaltitle}[1]{\renewcommand{\anvil@title}{#1}}
\newcommand{\proposalsubtitle}[1]{\renewcommand{\anvil@subtitle}{#1}}
\newcommand{\proposaldate}[1]{\renewcommand{\anvil@date}{#1}}
\newcommand{\proposalstage}[1]{\renewcommand{\anvil@stage}{#1}}

%% \maketitleblock — renders the studio kicker, title, subtitle, accent rule,
%% and the date/stage line. Mirrors the Gossamer LAN title block.
\newcommand{\maketitleblock}{%
  \thispagestyle{empty}%
  \begin{flushleft}%
    {\color{muted}\footnotesize\textsf{\MakeUppercase{\anvil@studio}}}\\[0.8em]%
    {\fontsize{26}{30}\selectfont\bfseries\color{ink}\anvil@title}\\[0.3em]%
    \ifx\anvil@subtitle\empty\else
      {\large\color{muted}\itshape\anvil@subtitle}\\[0.7em]%
    \fi
    {\color{rule}\rule{\textwidth}{2pt}}\\[0.2em]%
    {\footnotesize\color{muted}\anvil@date \hfill \anvil@stage}%
  \end{flushleft}%
  \vspace{0.4em}%
}

%% \herofigure{path} — full-width hero image. No-op when the argument is empty
%% (so a brief without a hero render still compiles).
\newcommand{\herofigure}[1]{%
  \def\anvil@hero{#1}%
  \ifx\anvil@hero\empty\else
    \begin{figure}[h]\centering
      \includegraphics[width=0.92\textwidth]{#1}%
    \end{figure}%
    \vspace{-0.6em}%
  \fi
}

%% \coda{text} — closing accent rule + italic muted coda line.
\newcommand{\coda}[1]{%
  \vfill
  {\color{rule}\rule{\textwidth}{1pt}}\\[0.2em]%
  {\footnotesize\itshape\color{muted}#1}%
}

\endinput
