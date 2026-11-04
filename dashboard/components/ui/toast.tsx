"use client";

import React, { createContext, useContext, useState } from "react";
import { CheckCircle2, AlertCircle, X } from "lucide-react";
import { cn } from "@/lib/utils";

interface Toast {
  id: string;
  type: "success" | "error" | "info";
  title: string;
  description?: string;
}

interface ToastContextValue {
  toast: (t: Omit<Toast, "id">) => void;
}

const ToastContext = createContext<ToastContextValue | undefined>(undefined);

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const toast = ({ type, title, description }: Omit<Toast, "id">) => {
    const id = Math.random().toString(36).substring(2, 9);
    setToasts((prev) => [...prev, { id, type, title, description }]);

    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, 4000);
  };

  const removeToast = (id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  };

  return (
    <ToastContext.Provider value={{ toast }}>
      {children}
      <div className="fixed bottom-4 right-4 z-50 flex flex-col gap-2 max-w-sm w-full pointer-events-none">
        {toasts.map((t) => (
          <div
            key={t.id}
            className={cn(
              "pointer-events-auto flex items-start gap-3 p-3.5 rounded-card border shadow-md bg-surface text-foreground transition-all duration-200",
              t.type === "error" ? "border-[#FECACA]" : "border-border"
            )}
          >
            {t.type === "success" && (
              <CheckCircle2 className="w-4 h-4 text-[#15803D] mt-0.5 shrink-0" />
            )}
            {t.type === "error" && (
              <AlertCircle className="w-4 h-4 text-[#B91C1C] mt-0.5 shrink-0" />
            )}
            {t.type === "info" && (
              <div className="w-2 h-2 rounded-full bg-foreground mt-1.5 shrink-0" />
            )}
            <div className="flex-1 min-w-0">
              <p className="text-xs font-semibold text-foreground">{t.title}</p>
              {t.description && (
                <p className="text-xs text-foreground-secondary mt-0.5 leading-relaxed">
                  {t.description}
                </p>
              )}
            </div>
            <button
              onClick={() => removeToast(t.id)}
              className="text-foreground-muted hover:text-foreground p-0.5 rounded transition-colors"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

export function useToast() {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    throw new Error("useToast must be used within ToastProvider");
  }
  return ctx;
}

