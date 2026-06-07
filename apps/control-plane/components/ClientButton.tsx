"use client";

import React, { useState } from 'react';

interface ClientButtonProps {
  label: string;
  action: () => Promise<void>;
  variant?: 'primary' | 'secondary' | 'danger' | 'success';
}

export default function ClientButton({ label, action, variant = 'primary' }: ClientButtonProps) {
  const [loading, setLoading] = useState(false);
  const [status, setStatus] = useState<null | 'success' | 'error'>(null);

  const handleClick = async () => {
    setLoading(true);
    setStatus(null);
    try {
      await action();
      setStatus('success');
      setTimeout(() => setStatus(null), 3000);
    } catch (error) {
      console.error(error);
      setStatus('error');
      setTimeout(() => setStatus(null), 5000);
    } finally {
      setLoading(false);
    }
  };

  const variantClasses = {
    primary: 'bg-indigo-600 hover:bg-indigo-700 text-white',
    secondary: 'border border-slate-700 hover:bg-slate-800 text-slate-300',
    danger: 'bg-red-600 hover:bg-red-700 text-white',
    success: 'bg-emerald-600 hover:bg-emerald-700 text-white'
  };

  return (
    <button
      onClick={handleClick}
      disabled={loading}
      className={`px-4 py-2 rounded-lg font-semibold transition-all flex items-center justify-center gap-2 ${variantClasses[variant]} ${loading ? 'opacity-50 cursor-not-allowed' : ''}`}
    >
      {loading ? (
        <svg className="animate-spin h-4 w-4" viewBox="0 0 24 24">
          <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
          <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
        </svg>
      ) : null}
      {status === 'success' && <span className="text-emerald-400">✓</span>}
      {status === 'error' && <span className="text-red-400">!</span>}
      {label}
    </button>
  );
}
