import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

// ===== TYPES =====

interface ApprovalRequest {
  request_id: string;
  command: string;
  executable: string;
  resolved_path: string | null;
  working_dir: string | null;
  timestamp: number;
  reason: string | null;
}

type ApprovalDecision = 'allow_once' | 'allow_always' | 'allow_session' | 'deny';

interface TerminalApprovalDialogProps {
  onRequestHandled?: () => void;
}

// ===== COMPONENT =====

export const TerminalApprovalDialog: React.FC<TerminalApprovalDialogProps> = ({
  onRequestHandled,
}) => {
  const [pendingRequest, setPendingRequest] = useState<ApprovalRequest | null>(null);
  const [isProcessing, setIsProcessing] = useState(false);
  const [rememberChoice, setRememberChoice] = useState(false);

  // Listen for approval requests from Tauri
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;

    const setupListener = async () => {
      unlisten = await listen<ApprovalRequest>('terminal-approval-request', (event) => {
        setPendingRequest(event.payload);
      });
    };

    setupListener();

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, []);

  // Handle approval decision
  const handleDecision = async (decision: ApprovalDecision) => {
    if (!pendingRequest || isProcessing) return;

    setIsProcessing(true);

    try {
      // Map our decision types to the Tauri command format
      // The Rust side expects: "allow", "allow_session", or "deny"
      // And a separate allow_always flag
      let tauriDecision: string;
      let allowAlways = false;
      
      switch (decision) {
        case 'allow_always':
          tauriDecision = 'allow';
          allowAlways = true;
          break;
        case 'allow_once':
          tauriDecision = 'allow';
          allowAlways = false;
          break;
        case 'allow_session':
          tauriDecision = 'allow_session';
          break;
        case 'deny':
        default:
          tauriDecision = 'deny';
          break;
      }
      
      await invoke('respond_terminal_approval', {
        requestId: pendingRequest.request_id,
        decision: tauriDecision,
        allowAlways: allowAlways,
      });

      setPendingRequest(null);
      setRememberChoice(false);
      onRequestHandled?.();
    } catch (error) {
      console.error('Failed to resolve approval:', error);
    } finally {
      setIsProcessing(false);
    }
  };

  // Handle keyboard shortcuts
  useEffect(() => {
    if (!pendingRequest) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        handleDecision('deny');
      } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        handleDecision(rememberChoice ? 'allow_always' : 'allow_once');
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [pendingRequest, rememberChoice]);

  if (!pendingRequest) {
    return null;
  }

  const formatTimestamp = (ts: number) => {
    return new Date(ts).toLocaleTimeString();
  };

  const truncateCommand = (cmd: string, maxLength = 100) => {
    if (cmd.length <= maxLength) return cmd;
    return cmd.substring(0, maxLength) + '...';
  };

  return (
    <div style={styles.overlay}>
      <div style={styles.dialog}>
        {/* Header */}
        <div style={styles.header}>
          <div style={styles.iconContainer}>
            <TerminalIcon />
          </div>
          <div style={styles.headerText}>
            <h2 style={styles.title}>Terminal Command Approval</h2>
            <p style={styles.subtitle}>
              The agent wants to run a terminal command
            </p>
          </div>
        </div>

        {/* Command Details */}
        <div style={styles.content}>
          {/* Command Preview */}
          <div style={styles.commandBox}>
            <code style={styles.commandText}>{pendingRequest.command}</code>
          </div>

          {/* Details */}
          <div style={styles.detailsGrid}>
            <div style={styles.detailRow}>
              <span style={styles.detailLabel}>Executable:</span>
              <span style={styles.detailValue}>
                {pendingRequest.resolved_path || pendingRequest.executable}
              </span>
            </div>
            
            {pendingRequest.working_dir && (
              <div style={styles.detailRow}>
                <span style={styles.detailLabel}>Working Directory:</span>
                <span style={styles.detailValue}>{pendingRequest.working_dir}</span>
              </div>
            )}
            
            {pendingRequest.reason && (
              <div style={styles.detailRow}>
                <span style={styles.detailLabel}>Reason:</span>
                <span style={styles.detailValue}>{pendingRequest.reason}</span>
              </div>
            )}
            
            <div style={styles.detailRow}>
              <span style={styles.detailLabel}>Requested at:</span>
              <span style={styles.detailValue}>
                {formatTimestamp(pendingRequest.timestamp)}
              </span>
            </div>
          </div>

          {/* Remember Choice Checkbox */}
          <label style={styles.checkboxLabel}>
            <input
              type="checkbox"
              checked={rememberChoice}
              onChange={(e) => setRememberChoice(e.target.checked)}
              style={styles.checkbox}
            />
            <span>Remember my choice for <code>{pendingRequest.executable}</code></span>
          </label>
        </div>

        {/* Actions */}
        <div style={styles.actions}>
          <button
            onClick={() => handleDecision('deny')}
            disabled={isProcessing}
            style={styles.denyButton}
          >
            <span>✗</span>
            <span>Deny</span>
          </button>
          
          <button
            onClick={() => handleDecision('allow_session')}
            disabled={isProcessing}
            style={styles.sessionButton}
          >
            <span>🔒</span>
            <span>This Session</span>
          </button>
          
          <button
            onClick={() => handleDecision(rememberChoice ? 'allow_always' : 'allow_once')}
            disabled={isProcessing}
            style={styles.allowButton}
          >
            <span>✓</span>
            <span>{rememberChoice ? 'Allow Always' : 'Allow Once'}</span>
          </button>
        </div>

        {/* Keyboard Hints */}
        <div style={styles.hints}>
          <span><kbd>Esc</kbd> Deny</span>
          <span><kbd>⌘</kbd>+<kbd>Enter</kbd> Allow</span>
        </div>
      </div>
    </div>
  );
};

// ===== ICONS =====

const TerminalIcon: React.FC = () => (
  <svg
    width="24"
    height="24"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <polyline points="4 17 10 11 4 5" />
    <line x1="12" y1="19" x2="20" y2="19" />
  </svg>
);

// ===== STYLES =====

const styles: { [key: string]: React.CSSProperties } = {
  overlay: {
    position: 'fixed',
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    backgroundColor: 'rgba(0, 0, 0, 0.6)',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    zIndex: 9999,
    backdropFilter: 'blur(4px)',
  },
  dialog: {
    backgroundColor: '#1a1a2e',
    borderRadius: '12px',
    boxShadow: '0 25px 50px -12px rgba(0, 0, 0, 0.5)',
    border: '1px solid rgba(255, 255, 255, 0.1)',
    maxWidth: '540px',
    width: '90%',
    overflow: 'hidden',
  },
  header: {
    display: 'flex',
    alignItems: 'center',
    gap: '16px',
    padding: '20px 24px',
    borderBottom: '1px solid rgba(255, 255, 255, 0.1)',
    backgroundColor: 'rgba(255, 165, 0, 0.1)',
  },
  iconContainer: {
    width: '48px',
    height: '48px',
    borderRadius: '12px',
    backgroundColor: 'rgba(255, 165, 0, 0.2)',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    color: '#ffa500',
  },
  headerText: {
    flex: 1,
  },
  title: {
    margin: 0,
    fontSize: '18px',
    fontWeight: 600,
    color: '#ffffff',
  },
  subtitle: {
    margin: '4px 0 0',
    fontSize: '14px',
    color: 'rgba(255, 255, 255, 0.6)',
  },
  content: {
    padding: '24px',
  },
  commandBox: {
    backgroundColor: '#0d0d1a',
    borderRadius: '8px',
    padding: '16px',
    border: '1px solid rgba(255, 255, 255, 0.1)',
    marginBottom: '20px',
    overflowX: 'auto',
  },
  commandText: {
    fontFamily: 'SF Mono, Menlo, Monaco, Consolas, monospace',
    fontSize: '14px',
    color: '#00ff88',
    wordBreak: 'break-all',
    whiteSpace: 'pre-wrap',
  },
  detailsGrid: {
    display: 'flex',
    flexDirection: 'column',
    gap: '12px',
    marginBottom: '20px',
  },
  detailRow: {
    display: 'flex',
    flexDirection: 'column',
    gap: '4px',
  },
  detailLabel: {
    fontSize: '12px',
    fontWeight: 500,
    color: 'rgba(255, 255, 255, 0.5)',
    textTransform: 'uppercase',
    letterSpacing: '0.5px',
  },
  detailValue: {
    fontSize: '14px',
    color: 'rgba(255, 255, 255, 0.9)',
    fontFamily: 'SF Mono, Menlo, Monaco, Consolas, monospace',
    wordBreak: 'break-all',
  },
  checkboxLabel: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    fontSize: '14px',
    color: 'rgba(255, 255, 255, 0.8)',
    cursor: 'pointer',
  },
  checkbox: {
    width: '18px',
    height: '18px',
    accentColor: '#00ff88',
  },
  actions: {
    display: 'flex',
    gap: '12px',
    padding: '20px 24px',
    borderTop: '1px solid rgba(255, 255, 255, 0.1)',
    backgroundColor: 'rgba(0, 0, 0, 0.2)',
  },
  denyButton: {
    flex: 1,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: '8px',
    padding: '12px 16px',
    border: 'none',
    borderRadius: '8px',
    backgroundColor: 'rgba(255, 68, 68, 0.2)',
    color: '#ff4444',
    fontSize: '14px',
    fontWeight: 500,
    cursor: 'pointer',
    transition: 'all 0.2s',
  },
  sessionButton: {
    flex: 1,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: '8px',
    padding: '12px 16px',
    border: '1px solid rgba(255, 255, 255, 0.2)',
    borderRadius: '8px',
    backgroundColor: 'transparent',
    color: 'rgba(255, 255, 255, 0.9)',
    fontSize: '14px',
    fontWeight: 500,
    cursor: 'pointer',
    transition: 'all 0.2s',
  },
  allowButton: {
    flex: 1,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: '8px',
    padding: '12px 16px',
    border: 'none',
    borderRadius: '8px',
    backgroundColor: '#00ff88',
    color: '#000000',
    fontSize: '14px',
    fontWeight: 600,
    cursor: 'pointer',
    transition: 'all 0.2s',
  },
  hints: {
    display: 'flex',
    justifyContent: 'center',
    gap: '24px',
    padding: '12px 24px',
    borderTop: '1px solid rgba(255, 255, 255, 0.05)',
    fontSize: '12px',
    color: 'rgba(255, 255, 255, 0.4)',
  },
};

export default TerminalApprovalDialog;
