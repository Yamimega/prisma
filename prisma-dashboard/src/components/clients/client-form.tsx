"use client";

import { useState } from "react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";

interface ClientFormProps {
  onSubmit: (name: string) => void;
  isLoading: boolean;
}

export function ClientForm({ onSubmit, isLoading }: ClientFormProps) {
  const [name, setName] = useState("");

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    onSubmit(name.trim());
    setName("");
  }

  return (
    <form onSubmit={handleSubmit} className="flex items-end gap-3">
      <div className="grid w-full max-w-sm gap-1.5">
        <Label htmlFor="client-name">Client Name</Label>
        <Input
          id="client-name"
          type="text"
          placeholder="Enter client name"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </div>
      <Button type="submit" disabled={isLoading}>
        {isLoading ? "Creating..." : "Create Client"}
      </Button>
    </form>
  );
}
