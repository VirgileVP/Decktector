import { useEffect, useId, useRef, type ReactNode } from 'react'

interface ConfirmDialogProps {
  title: string
  children: ReactNode
  confirmLabel: string
  cancelLabel?: string
  onConfirm: () => void
  onCancel: () => void
}

/**
 * Modal asking the user to confirm a choice before it takes effect. Cancel is
 * the focused default, so a stray Enter never confirms. Escape and backdrop
 * clicks cancel without reaching an enclosing dialog (e.g. Settings), which
 * would otherwise close along with this one.
 */
export function ConfirmDialog({
  title,
  children,
  confirmLabel,
  cancelLabel = 'Cancel',
  onConfirm,
  onCancel
}: ConfirmDialogProps): React.JSX.Element {
  const cancelRef = useRef<HTMLButtonElement>(null)
  const titleId = useId()
  const bodyId = useId()

  useEffect(() => {
    cancelRef.current?.focus()
  }, [])

  const handleKeyDown = (event: React.KeyboardEvent): void => {
    if (event.key !== 'Escape') return
    event.stopPropagation()
    onCancel()
  }

  return (
    <div
      className="confirm-backdrop"
      onClick={(event) => {
        event.stopPropagation()
        onCancel()
      }}
    >
      <div
        className="confirm-panel"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={bodyId}
        onClick={(event) => event.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        <h3 id={titleId} className="confirm-title">
          {title}
        </h3>
        <div id={bodyId} className="confirm-body">
          {children}
        </div>
        <div className="confirm-actions">
          <button ref={cancelRef} type="button" className="setting-btn" onClick={onCancel}>
            {cancelLabel}
          </button>
          <button type="button" className="setting-btn confirm-accept" onClick={onConfirm}>
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  )
}
