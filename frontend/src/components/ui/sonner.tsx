"use client"

import {
  CircleCheckIcon,
  InfoIcon,
  Loader2Icon,
  OctagonXIcon,
  TriangleAlertIcon,
} from "lucide-react"
import { Toaster as Sonner, type ToasterProps } from "sonner"

const Toaster = ({ ...props }: ToasterProps) => {
  return (
    <Sonner
      theme="light"
      className="toaster group"
      icons={{
        success: <CircleCheckIcon className="size-4" />,
        info: <InfoIcon className="size-4" />,
        warning: <TriangleAlertIcon className="size-4" />,
        error: <OctagonXIcon className="size-4" />,
        loading: <Loader2Icon className="size-4 animate-spin" />,
      }}
      style={
        {
          "--normal-bg": "#FFFFFF",
          "--normal-text": "#1A1A19",
          "--normal-border": "#E6E4DF",
          "--border-radius": "10px",
          "--success-bg": "#DAE9E0",
          "--success-text": "#2D6A4F",
          "--success-border": "#2D6A4F",
          "--error-bg": "#F5E0DD",
          "--error-text": "#B44C3F",
          "--error-border": "#B44C3F",
        } as React.CSSProperties
      }
      {...props}
    />
  )
}

export { Toaster }
