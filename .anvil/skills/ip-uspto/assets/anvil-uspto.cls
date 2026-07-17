%%
%% anvil-uspto.cls — USPTO non-provisional utility patent application class
%%
%% Provides:
%%   - 1-inch margins all sides, US Letter
%%   - 12pt Times Roman, 1.5 line spacing
%%   - Auto-numbered paragraphs in [0001] style via \anvilpara{...}
%%   - Reference numeral macro \refnum{...}
%%   - Section heading commands for 37 CFR 1.77(b) headings
%%   - Claim environment with auto-numbering and proper indentation
%%   - Abstract section heading
%%
%% Conventions:
%%   - Section headings are uppercase (37 CFR 1.77(b) practice).
%%   - Paragraphs in body sections are numbered [0001], [0002], ... using \anvilpara.
%%   - Reference numerals in spec are wrapped in \refnum{N} for grep-ability by pre-flight.
%%
%% Usage:
%%   \documentclass{anvil-uspto}
%%   \title{Title of the Invention}
%%   \author{Inventor One \and Inventor Two}
%%   \begin{document}
%%     \maketitle
%%     \fieldoftheinvention
%%     \anvilpara{...}
%%     \background
%%     \anvilpara{...}
%%     ...
%%     \begin{claims}
%%       \begin{claim} ... \end{claim}
%%       \begin{claim} ... \end{claim}
%%     \end{claims}
%%     \abstract
%%     ...
%%   \end{document}
%%
\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{anvil-uspto}[2026/05/28 v0.1 Anvil USPTO patent application class]

%% Base class — article with US Letter, 12pt
\LoadClass[12pt,letterpaper,oneside]{article}

%% Geometry — 1-inch margins per 37 CFR 1.52(a)(1)(ii)
\RequirePackage[margin=1in,letterpaper]{geometry}

%% Line spacing — 1.5 spacing per 37 CFR 1.52(b)(2)(ii) practice
\RequirePackage{setspace}
\onehalfspacing

%% Font — Times Roman (12pt set by class option)
\RequirePackage{times}

%% Paragraph numbering counter
\newcounter{anvilpara}
\setcounter{anvilpara}{0}

%% \anvilpara{text} — numbered paragraph in [NNNN] style
\newcommand{\anvilpara}[1]{%
  \stepcounter{anvilpara}%
  \par\noindent[\anvilparanum] #1\par
}

%% Format counter as [NNNN] zero-padded
\newcommand{\anvilparanum}{%
  \ifnum\value{anvilpara}<10 000\arabic{anvilpara}%
  \else\ifnum\value{anvilpara}<100 00\arabic{anvilpara}%
  \else\ifnum\value{anvilpara}<1000 0\arabic{anvilpara}%
  \else\arabic{anvilpara}%
  \fi\fi\fi%
}

%% \refnum{N} — reference numeral; emits N as-is, but the wrapper enables
%% grep-based extraction by the pre-flight check and reference-numeral audit.
\newcommand{\refnum}[1]{#1}

%% Section heading commands per 37 CFR 1.77(b)
\newcommand{\fieldoftheinvention}{\section*{FIELD OF THE INVENTION}}
\newcommand{\background}{\section*{BACKGROUND OF THE INVENTION}}
\newcommand{\summary}{\section*{SUMMARY OF THE INVENTION}}
\newcommand{\briefdescriptionofdrawings}{\section*{BRIEF DESCRIPTION OF THE DRAWINGS}}
\newcommand{\detaileddescription}{\section*{DETAILED DESCRIPTION OF EMBODIMENTS}}

%% Claims block — \begin{claims}...\end{claims} (an environment so it nests
%% properly with claim items).
\newenvironment{claims}{%
  \section*{CLAIMS}%
  \begin{enumerate}%
    \renewcommand{\theenumi}{\arabic{enumi}}%
}{%
  \end{enumerate}%
}

%% Claim environment — \begin{claim}...\end{claim} — one item per claim within
%% the surrounding claims block.
\newenvironment{claim}{%
  \item\setlength{\parindent}{0pt}%
}{%
  \par
}

%% Abstract — separate page. Class provides the heading; word-count
%% advisory is left to the pre-flight check.
\renewcommand{\abstract}{\newpage\section*{ABSTRACT OF THE DISCLOSURE}}

%% Title page formatting — keep simple, USPTO does not require fancy formatting.
%% Locally redefine \and to a comma so multi-inventor \author{A \and B}
%% lists render cleanly inside \begin{center} (the article default uses
%% \and as a tabular column separator, which fails outside tabular mode).
\renewcommand{\maketitle}{%
  \begingroup
  \renewcommand{\and}{, }%
  \begin{center}%
    {\Large\bfseries\@title\par}%
    \vspace{1em}%
    {\@author\par}%
  \end{center}%
  \vspace{2em}%
  \endgroup
}

%% End of class
\endinput
