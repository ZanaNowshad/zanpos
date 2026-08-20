import { WORKFLOW_CATALOGUE, WORKFLOW_GROUPS } from "./workflowCatalogue";

interface Props {
  /** Sends the workflow's prompt as if the operator had typed it. */
  onRun: (prompt: string) => void;
  heading: string;
  subheading: string;
}

/**
 * The opening screen of ZanAI: the 21 procedures it already knows.
 *
 * The assistant used to open on an empty composer, which asks the shopkeeper
 * to guess both what it can do and how to phrase it — on a 1024x768 till whose
 * keyboard is on screen. The procedures were already written and named in
 * `workflows.rs`; this puts them where the blank box was.
 *
 * Shown only before the first message. Once there is a conversation the
 * transcript is the more useful thing to look at, and these are one tap away
 * again as soon as it is cleared.
 */
export default function WorkflowLauncher({ onRun, heading, subheading }: Props) {
  return (
    <div className="oa-wf-launcher">
      <div className="oa-wf-intro">
        <h2>{heading}</h2>
        <p>{subheading}</p>
      </div>
      {WORKFLOW_GROUPS.map(group => {
        const entries = WORKFLOW_CATALOGUE.filter(w => w.group === group.id);
        if (entries.length === 0) return null;
        return (
          <section key={group.id} className="oa-wf-group" aria-label={group.label}>
            <h3 className="oa-wf-group-label">{group.label}</h3>
            <div className="oa-wf-grid">
              {entries.map(w => (
                <button
                  key={w.id}
                  type="button"
                  className="oa-wf-card"
                  onClick={() => onRun(w.prompt)}
                >
                  <span className="oa-wf-card-label">{w.label}</span>
                  <span className="oa-wf-card-blurb">{w.blurb}</span>
                </button>
              ))}
            </div>
          </section>
        );
      })}
    </div>
  );
}
