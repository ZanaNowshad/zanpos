import { useCallback, useEffect, useRef, useState } from "react";
import { adminSaveTaxRule, appConfigLoad, authLoginPin, onboardingGetState, onboardingMarkStep, setupWizardComplete } from "../../tauri/commands";
import type { AppConfig } from "../../types";
import { ONBOARDING_STEPS, nextIncompleteStepIndex, stepKeyAt, type OnboardingStepKey } from "./onboardingSteps";
import StepIdentity, { DEFAULT_IDENTITY_DRAFT, type IdentityDraft } from "./StepIdentity";
import StepOwnerPin, { type OwnerDraft } from "./StepOwnerPin";
import StepWhatsApp from "./StepWhatsApp";
import StepProducts from "./StepProducts";
import StepPrinter from "./StepPrinter";
import StepGoLive from "./StepGoLive";
import StepRestore from "./StepRestore";

interface Props {
  /** Called once every step is resolved, with the config the app should adopt.
   *  Matches SetupWizard's contract so this can stand in as the "new store"
   *  path without App.tsx knowing which flow ran. */
  onComplete: (config: AppConfig) => void;
}

/** Owner identity carried across steps 3-6. Steps below need a real user id
 * for their RBAC-checked commands, so nothing after the PIN step can run
 * until this exists. */
interface OwnerSession {
  userId: string;
  username: string;
}

/**
 * Persists the VAT choice made in step 1.
 *
 * It cannot be written with the store itself: `setup_wizard_complete` takes no
 * tax fields, and saving a rule requires an actor id that only exists once the
 * owner account has been created. So it runs immediately after sign-in instead.
 * "standard" is left alone deliberately — setup already seeds the default rule,
 * and writing a second one would leave two competing standard rates.
 */
async function applyVatChoice(identity: IdentityDraft, ownerUserId: string) {
  if (identity.vatChoice === "standard") return;
  const ratePercent = identity.vatChoice === "none"
    ? 0
    : Number.parseFloat(identity.customRatePercent);
  if (!Number.isFinite(ratePercent) || ratePercent < 0) return;
  await adminSaveTaxRule({
    name: identity.vatChoice === "none" ? "No VAT" : `VAT ${ratePercent}%`,
    // Basis points: 10% -> 1000. Rounded because a fractional basis point is
    // not representable and would silently truncate on the way to SQLite.
    rate_basis_points: Math.round(ratePercent * 100),
    inclusive: true,
    is_active: true,
    actor_user_id: ownerUserId,
  });
}

/**
 * First-run onboarding orchestrator — sequences the six steps defined in
 * `onboardingSteps.ts` and owns everything the individual steps must not:
 * the single atomic store+owner write, progress persistence, and resume.
 *
 * Resume is the reason progress is written to `onboarding_state` after every
 * step rather than at the end: power cuts are routine in this market, and a
 * wizard that restarts from zero is worse than no wizard at all.
 */
export default function OnboardingWizard({ onComplete }: Props) {
  const [loading, setLoading] = useState(true);
  const [stepIndex, setStepIndex] = useState(0);
  const [identity, setIdentity] = useState<IdentityDraft>(DEFAULT_IDENTITY_DRAFT);
  const [owner, setOwner] = useState<OwnerSession | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Step 0 — offered before anything is created, because someone restoring a
  // dead till should not have to type their catalogue in again first. Skipped
  // automatically when resuming, since the store already exists by then.
  const [restoreOffered, setRestoreOffered] = useState(false);
  // Shown after the last step instead of dropping straight onto the POS: the
  // moment the wizard finishes is the one place to say plainly that the store
  // is ready to trade.
  const [done, setDone] = useState(false);
  // Captured from setup_wizard_complete. Absent when resuming a wizard that
  // created the store on an earlier run, so completion re-reads it.
  const configRef = useRef<AppConfig | null>(null);

  // Resolves the config now but hands control to the exit screen; onComplete
  // fires only when the operator acknowledges it.
  const finish = useCallback(async () => {
    configRef.current = configRef.current ?? await appConfigLoad();
    setDone(true);
  }, []);

  const leaveWizard = useCallback(async () => {
    const config = configRef.current ?? await appConfigLoad();
    onComplete(config);
  }, [onComplete]);

  // Resume where the last run stopped. A failure here is non-fatal: starting
  // at step 1 is always safe, and each step is individually idempotent.
  useEffect(() => {
    let cancelled = false;
    onboardingGetState()
      .then(rows => {
        if (cancelled) return;
        setStepIndex(nextIncompleteStepIndex(new Set(rows.map(r => r.step))));
        // Only offer restore on a genuinely fresh machine. Resuming means a
        // store already exists here, and restoring over it is not what the
        // operator is trying to do.
        setRestoreOffered(rows.length > 0);
      })
      .catch(() => { /* non-fatal — fall back to step 1 */ })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, []);

  const markDone = useCallback(async (key: OnboardingStepKey, actorUserId: string) => {
    // Progress is a convenience, never a blocker — a store mid-setup must be
    // able to keep moving even if this write fails.
    try { await onboardingMarkStep(key, actorUserId); } catch { /* non-fatal */ }
  }, []);

  const advance = useCallback(async (key: OnboardingStepKey, actorUserId: string) => {
    await markDone(key, actorUserId);
    setStepIndex(current => {
      const next = current + 1;
      if (next >= ONBOARDING_STEPS.length) void finish();
      return Math.min(next, ONBOARDING_STEPS.length - 1);
    });
  }, [markDone, finish]);

  /** Steps 1 and 2 commit together: the store row and the owner account are
   * one atomic `setup_wizard_complete` call, so a half-created store can never
   * exist. The owner is then signed in with the PIN they just chose, which is
   * what gives the remaining steps an actor id. */
  const submitOwner = useCallback(async (draft: OwnerDraft) => {
    setSubmitting(true);
    setError(null);
    try {
      configRef.current = await setupWizardComplete({
        store_name: identity.storeName,
        store_phone: identity.phone,
        currency: identity.currency,
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
        owner_display_name: draft.ownerName,
        owner_username: draft.ownerUsername,
        owner_pin: draft.ownerPin,
      });
      const session = await authLoginPin(draft.ownerUsername, draft.ownerPin);
      const next: OwnerSession = { userId: session.user_id, username: draft.ownerUsername };
      setOwner(next);
      await applyVatChoice(identity, next.userId);
      await markDone("identity", next.userId);
      await advance("owner_pin", next.userId);
    } catch (e) {
      setError(String(e));
    } finally {
      setSubmitting(false);
    }
  }, [identity, markDone, advance]);

  /** Re-authentication after a resume. The owner already exists — this run of
   * the wizard just has no session for them, so steps 3-6 have no actor id. */
  const reauth = useCallback(async (username: string, pin: string) => {
    setSubmitting(true);
    setError(null);
    try {
      const session = await authLoginPin(username, pin);
      setOwner({ userId: session.user_id, username });
    } catch (e) {
      setError(String(e));
    } finally {
      setSubmitting(false);
    }
  }, []);

  if (loading) return <div className="setup-screen"><div className="setup-panel">Loading…</div></div>;

  // Step 0 — before the progress bar, because restoring replaces setup rather
  // than being part of it.
  if (!restoreOffered) {
    return (
      <div className="setup-screen">
        <div className="setup-panel">
          <StepRestore
            ownerUserId={owner?.userId ?? ""}
            onRestored={() => setRestoreOffered(true)}
            onSkip={() => setRestoreOffered(true)}
          />
        </div>
      </div>
    );
  }

  if (done) {
    return (
      <div className="setup-screen">
        <div className="setup-panel">
          <div className="setup-content setup-done">
            <div className="setup-logo">ZAN<span>POS</span></div>
            <h1 className="setup-title">You're ready to sell</h1>
            <p className="setup-body">
              Your store is set up. The till opens in practice mode so your
              cashier can ring up a sale without it counting — the badge at the
              top says so, and the first real sale clears it.
            </p>
            <div className="setup-actions">
              <button className="setup-btn-primary setup-btn-finish" onClick={() => void leaveWizard()}>
                Open the till
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  const activeKey = stepKeyAt(stepIndex);
  const needsOwner = stepIndex > 1 && owner === null;

  return (
    <div className="setup-screen">
      <div className="setup-progress">
        <div className="setup-progress-line" />
        {ONBOARDING_STEPS.map((step, index) => (
          <div
            key={step.key}
            className={`setup-step-dot${index < stepIndex ? " setup-step-done" : ""}${index === stepIndex ? " setup-step-active" : ""}`}
          >
            <div className="setup-dot-circle">{index + 1}</div>
            <div className="setup-dot-label">{step.label}</div>
          </div>
        ))}
      </div>

      <div className="setup-panel">
        {needsOwner ? (
          <ResumeSignIn submitting={submitting} error={error} onSubmit={reauth} />
        ) : activeKey === "identity" ? (
          <StepIdentity
            draft={identity}
            onNext={draft => { setIdentity(draft); setStepIndex(1); }}
          />
        ) : activeKey === "owner_pin" ? (
          <StepOwnerPin
            storeName={identity.storeName}
            submitting={submitting}
            error={error}
            onSubmit={submitOwner}
            onBack={() => setStepIndex(0)}
          />
        ) : activeKey === "whatsapp" ? (
          <StepWhatsApp ownerUserId={owner!.userId} onDone={() => void advance("whatsapp", owner!.userId)} />
        ) : activeKey === "products" ? (
          <StepProducts
            ownerUserId={owner!.userId}
            ownerUsername={owner!.username}
            onDone={() => void advance("products", owner!.userId)}
          />
        ) : activeKey === "printer" ? (
          <StepPrinter ownerUserId={owner!.userId} onDone={() => void advance("printer", owner!.userId)} />
        ) : (
          <StepGoLive
            ownerUserId={owner!.userId}
            language={identity.language}
            onDone={() => void advance("golive", owner!.userId)}
          />
        )}
      </div>
    </div>
  );
}

function ResumeSignIn({ submitting, error, onSubmit }: {
  submitting: boolean;
  error: string | null;
  onSubmit: (username: string, pin: string) => void;
}) {
  const [username, setUsername] = useState("admin");
  const [pin, setPin] = useState("");

  return (
    <div className="setup-content">
      <div className="setup-title">Welcome back</div>
      <div className="setup-body">
        Your store is already set up. Sign in as the owner to finish the remaining steps.
      </div>
      <label>
        Username
        <input value={username} onChange={e => setUsername(e.target.value)} autoComplete="off" />
      </label>
      <label>
        Owner PIN
        <input
          className="setup-pin-input"
          type="password"
          inputMode="numeric"
          value={pin}
          onChange={e => setPin(e.target.value.replace(/\D/g, ""))}
          autoComplete="off"
        />
      </label>
      {error && <div className="setup-body" role="alert">{error}</div>}
      <div className="setup-actions">
        <button
          className="setup-btn-primary"
          disabled={submitting || pin.length < 4 || !username.trim()}
          onClick={() => onSubmit(username.trim(), pin)}
        >
          {submitting ? "Signing in…" : "Continue"}
        </button>
      </div>
    </div>
  );
}
