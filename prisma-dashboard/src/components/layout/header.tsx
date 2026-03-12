"use client";

import { signOut } from "next-auth/react";

interface HeaderProps {
  title: string;
}

export function Header({ title }: HeaderProps) {
  return (
    <header className="flex h-14 items-center justify-between border-b px-6">
      <h1 className="text-lg font-semibold">{title}</h1>

      <div className="flex items-center gap-4">
        <span className="text-sm text-muted-foreground">Admin</span>
        <button
          onClick={() => signOut()}
          className="text-sm text-muted-foreground transition-colors hover:text-foreground"
        >
          Sign out
        </button>
      </div>
    </header>
  );
}
