"""CSS + auto ToC. Run with ``uv run python examples/styled_report.py``."""

from pdfun import HtmlDocument

html = """
<style>
  @page { size: A4; margin: 2cm; }
  body { font-family: Helvetica; font-size: 11pt; line-height: 1.4; color: #222; }
  h1 { color: #1a237e; border-bottom: 2pt solid #1a237e; padding-bottom: 4pt; }
  h2 { color: #3949ab; margin-top: 18pt; page-break-after: avoid; }
  .note {
    background: #fff3e0;
    padding: 8pt;
    border-left: 3pt solid #ff9800;
    margin: 12pt 0;
  }
</style>

<h1>Quarterly Report</h1>
<p class="note">Figures are preliminary and subject to revision.</p>

<h2>Revenue</h2>
<p>Revenue grew 14% quarter-over-quarter, driven by expansion in enterprise accounts.</p>

<h2>Costs</h2>
<p>Operating costs held flat as hiring slowed. Infrastructure spend declined 6%.</p>

<h2>Outlook</h2>
<p>We expect modest acceleration in the next quarter as new contracts ramp.</p>
"""

HtmlDocument(string=html, toc="Contents").write_pdf("styled_report.pdf")
print("wrote styled_report.pdf")
