"use client";

import React from "react";
import { AlertTriangle, X } from "lucide-react";

interface DrainConfirmDialogProps {
  isOpen: boolean;
  onClose: () => void;
  onConfirm: () => void;
  backendAddr: string;
  isDraining: boolean;
  isLoading?: boolean;
}

export function DrainConfirmDialog({
  isOpen,
  onClose,
  onConfirm,
  backendAddr,
  isDraining,
  isLoading = false,
}: DrainConfirmDialogProps) {
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 backdrop-blur-[1px]">
      <div className="bg-surface border border-border rounded-card shadow-lg max-w-md w-full p-6 animate-in fade-in zoom-in-95 duration-150">
        <div className="flex items-start justify-between">
          <div className="flex items-center gap-3">
            <div className="w-9 h-9 rounded-full bg-[#FFFBEB] border border-[#FDE68A] flex items-center justify-center shrink-0">
              <AlertTriangle className="w-4 h-4 text-[#B45309]" />
            </div>
            <div>
              <h3 className="text-sm font-semibold text-foreground">
                {isDraining ? "Enable Backend Traffic" : "Drain Backend Node"}
              </h3>
              <p className="text-xs text-foreground-muted font-mono mt-0.5">{backendAddr}</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="text-foreground-muted hover:text-foreground p-1 rounded transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="mt-4 text-xs text-foreground-secondary leading-relaxed bg-surface-subtle p-3 rounded-btn border border-border">
          {isDraining ? (
            <p>
              Enabling this backend will immediately re-admit incoming traffic according to its
              assigned weight and configured balancing algorithm. If slow-start is enabled, traffic
              will ramp gradually.
            </p>
          ) : (
            <p>
              Draining will immediately exclude this backend from receiving new incoming requests.
              Currently active in-flight requests will be allowed to complete normally until the
              drain deadline elapses.
            </p>
          )}
        </div>

        <div className="mt-6 flex items-center justify-end gap-3">
          <button
            type="button"
            onClick={onClose}
            disabled={isLoading}
            className="px-3.5 py-1.5 rounded-btn border border-border text-xs font-medium text-foreground-secondary hover:text-foreground hover:bg-surface-subtle transition-colors"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={onConfirm}
            disabled={isLoading}
            className="px-3.5 py-1.5 rounded-btn bg-foreground text-primary-foreground hover:bg-primary-hover text-xs font-medium transition-colors disabled:opacity-50 flex items-center gap-2"
          >
            {isLoading ? "Processing..." : isDraining ? "Enable Traffic" : "Confirm Drain"}
          </button>
        </div>
      </div>
    </div>
  );
}

