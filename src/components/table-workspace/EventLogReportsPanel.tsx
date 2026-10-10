import { useEffect, useState } from "react";

import { ReportViewer } from "../shared/ReportViewer";
import { openTableEventLog, openTableReports } from "../../utils/ipc";
import type { AnyReport, TableEventLog } from "../../utils/ipc";

type Tab = "Event Log" | "Reports";

interface Props {
  tableId: string;
}

function reportEntry(report: AnyReport): { kind: string; reportId: string; title: string; state: string } {
  const [kind, content] = Object.entries(report)[0] as [string, { report_id: string; report_title: string; report_state: string }];
  return { kind, reportId: content.report_id, title: content.report_title, state: content.report_state };
}

// Single tabbed bottom panel — Event Log | Reports — each tab calling its
// own canonical command (table.open_event_log / table.open_reports).
// Every list here is read-only: no edit/delete/reorder affordance on any
// entry, matching INV-021.
export function EventLogReportsPanel({ tableId }: Props) {
  const [tab, setTab] = useState<Tab>("Event Log");
  const [eventLog, setEventLog] = useState<TableEventLog | null>(null);
  const [reports, setReports] = useState<AnyReport[] | null>(null);
  const [openReportId, setOpenReportId] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (tab === "Event Log") {
      openTableEventLog(tableId).then((log) => {
        if (!cancelled) setEventLog(log);
      });
    } else {
      openTableReports(tableId).then((r) => {
        if (!cancelled) setReports(r);
      });
    }
    return () => {
      cancelled = true;
    };
  }, [tableId, tab]);

  return (
    <section aria-label="Event Log and Reports">
      <div role="tablist" aria-label="Event Log and Reports tabs">
        <button type="button" role="tab" aria-selected={tab === "Event Log"} onClick={() => setTab("Event Log")}>
          Event Log
        </button>
        <button type="button" role="tab" aria-selected={tab === "Reports"} onClick={() => setTab("Reports")}>
          Reports
        </button>
      </div>
      {tab === "Event Log" && (
        <ul aria-label="Table Event Log">
          {(eventLog?.entries ?? []).map((entry) => (
            <li key={entry.id}>
              {entry.recorded_at}: {entry.event}
            </li>
          ))}
        </ul>
      )}
      {tab === "Reports" && (
        <ul aria-label="Reports">
          {(reports ?? []).map((report) => {
            const entry = reportEntry(report);
            return (
              <li key={entry.reportId}>
                <button type="button" onClick={() => setOpenReportId(entry.reportId)}>
                  {entry.kind}: {entry.title} ({entry.state})
                </button>
              </li>
            );
          })}
        </ul>
      )}

      {openReportId && <ReportViewer reportId={openReportId} onClose={() => setOpenReportId(null)} />}
    </section>
  );
}
