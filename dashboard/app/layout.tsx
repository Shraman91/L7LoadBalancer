import type { Metadata } from "next";
import "./globals.css";
import { Sidebar } from "@/components/layout/sidebar";
import { Providers } from "@/components/providers";

export const metadata: Metadata = {
  title: "l7lb Admin Dashboard",
  description: "Production Control Plane and Observability Dashboard for l7lb",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" className="h-full bg-background">
      <body className="h-full flex overflow-hidden text-foreground antialiased">
        <Providers>
          <Sidebar />
          <div className="flex-1 flex flex-col min-w-0 h-full overflow-hidden">
            {children}
          </div>
        </Providers>
      </body>
    </html>
  );
}

