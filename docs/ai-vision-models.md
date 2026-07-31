# ZANPOS — AI Vision Models (Reading WhatsApp Photos)

**Who this is for:** owners/managers who send a WhatsApp photo to the OfficeAI assistant
via the notification bell's **Send AI** button and get an error like
`no endpoint for images` or `this model does not support image input`.

---

## 1. Why Photos Sometimes Fail

The "Send AI" button already attaches the decrypted photo to the message and forwards it
to the assistant. The image travels correctly all the way to the model — see
[§4 The plumbing is already correct](#4-the-plumbing-is-already-correct).

The only thing that can fail is the **model itself**. Many fast/cheap text models are
**text-only** and reject any request that contains an image. The default
`deepseek-v4-flash` (via OpenRouter) is one of these: it has no vision endpoint, so the
photo is refused even though ZANPOS sent it.

**There is no code fix for this** — a text-only model physically cannot see images. You
must point ZANPOS at a **vision-capable** model.

---

## 2. Recommended Vision Models (OpenRouter)

Any of these read images and work with the existing OfficeAI tool flow:

| Model ID | Notes |
|----------|-------|
| `google/gemini-2.0-flash-exp:free` | Free tier, fast, strong at reading prices/labels off photos. Good first choice. |
| `openai/gpt-4o-mini` | Cheap, reliable vision + tool calling. Paid but very low cost. |
| `qwen/qwen-2-vl-7b-instruct` | Open-weight vision model, budget-friendly alternative. |

Tip: on OpenRouter you can confirm a model supports images by checking that its modality
is listed as **text+image** (not text-only) on its model page.

> If you use the **Gemini** provider directly (not via OpenRouter), `gemini-2.0-flash`
> is vision-capable out of the box. The **Anthropic / Claude** provider (e.g.
> `claude-sonnet-4-6`) also reads images natively.

---

## 3. How to Switch the Model (Settings → AI)

1. Open **OfficeAI** from the POS sidebar (owner or manager only).
2. Go to the **Settings** tab, then the **AI** section.
3. Find the provider/model configuration:
   - **OpenAI-compatible / OpenRouter:** set the **Model** field to one of the IDs in
     §2 (for example `google/gemini-2.0-flash-exp:free`). Leave the base URL as your
     OpenRouter endpoint (`https://openrouter.ai/api/v1`) and keep your existing API key.
   - **Gemini provider:** set the model to `gemini-2.0-flash`.
   - **Anthropic provider:** any current Claude model already supports images.
4. **Save.**
5. Return to the notification bell, open the photo message, and press **Send AI** again —
   the assistant will now receive and read the image.

No restart is required; the new model is picked up on the next message.

---

## 4. The Plumbing Is Already Correct

For maintainers: once a vision model is selected, **no other change is needed**. The
image path is wired end-to-end and verified:

- `NotificationModal.tsx` — `sendToAi` calls `whatsappGetMedia`, then
  `onSendToAi({ text, imageBase64, imageMediaType })`.
- `PosPage.tsx` — `onSendToAi` forwards the handoff to `onAskOfficeAI`.
- `App.tsx` — stores it in `officeAiInitialMessage`, passed to `OfficeAIPage` as
  `initialAiMessage`.
- `OfficeAIPage.tsx` — the handoff effect calls `ctrl.setImageAttachment(...)` and then
  `ctrl.handleSend(text)`.
- `useChatController.ts` — `handleSend` sends `image_base64` + `image_media_type` to the
  backend via `aiChatStream`.
- `src-tauri/src/ai/streaming.rs` — `build_messages` (Anthropic) and the OpenAI builder
  attach the image to the user turn; `openai_client.rs::user_msg_with_image` and
  `provider.rs::send_chat` pass it through in the correct vision format.

The error therefore originates at the provider, not in ZANPOS. Selecting a vision-capable
model is the complete fix.
