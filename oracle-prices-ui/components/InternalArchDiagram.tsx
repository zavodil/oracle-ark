'use client';

import { useEffect, useRef } from 'react';

export default function InternalArchDiagram() {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // Set canvas size
    const dpr = window.devicePixelRatio || 1;
    const width = 700;
    const height = 400;
    canvas.width = width * dpr;
    canvas.height = height * dpr;
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
    ctx.scale(dpr, dpr);

    // Clear
    ctx.fillStyle = '#0f1419';
    ctx.fillRect(0, 0, width, height);

    // Colors
    const colors = {
      primary: '#3b82f6',
      secondary: '#10b981',
      accent: '#f59e0b',
      tee: '#8b5cf6',
      push: '#ef4444',
      text: '#e5e7eb',
      muted: '#6b7280',
      border: '#374151',
    };

    // Draw rounded box
    const drawBox = (x: number, y: number, w: number, h: number, color: string, label: string, sublabel?: string, labelTop = false) => {
      ctx.strokeStyle = color;
      ctx.lineWidth = 2;
      ctx.fillStyle = `${color}15`;

      const r = 8;
      ctx.beginPath();
      ctx.moveTo(x + r, y);
      ctx.lineTo(x + w - r, y);
      ctx.quadraticCurveTo(x + w, y, x + w, y + r);
      ctx.lineTo(x + w, y + h - r);
      ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
      ctx.lineTo(x + r, y + h);
      ctx.quadraticCurveTo(x, y + h, x, y + h - r);
      ctx.lineTo(x, y + r);
      ctx.quadraticCurveTo(x, y, x + r, y);
      ctx.closePath();
      ctx.fill();
      ctx.stroke();

      ctx.fillStyle = colors.text;
      ctx.font = 'bold 13px system-ui, sans-serif';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';

      // Position label at top or center
      const labelY = labelTop ? y + 20 : y + h / 2 - (sublabel ? 8 : 0);
      ctx.fillText(label, x + w / 2, labelY);

      if (sublabel) {
        ctx.fillStyle = colors.muted;
        ctx.font = '11px system-ui, sans-serif';
        const sublabelY = labelTop ? y + 36 : y + h / 2 + 10;
        ctx.fillText(sublabel, x + w / 2, sublabelY);
      }
    };

    // Draw arrow
    const drawArrow = (fromX: number, fromY: number, toX: number, toY: number, color: string, label?: string, vertical = false) => {
      ctx.strokeStyle = color;
      ctx.fillStyle = color;
      ctx.lineWidth = 2;
      ctx.setLineDash([]);

      ctx.beginPath();
      ctx.moveTo(fromX, fromY);
      ctx.lineTo(toX, toY);
      ctx.stroke();

      // Arrowhead
      const angle = Math.atan2(toY - fromY, toX - fromX);
      const headLen = 8;
      ctx.beginPath();
      ctx.moveTo(toX, toY);
      ctx.lineTo(
        toX - headLen * Math.cos(angle - Math.PI / 6),
        toY - headLen * Math.sin(angle - Math.PI / 6)
      );
      ctx.lineTo(
        toX - headLen * Math.cos(angle + Math.PI / 6),
        toY - headLen * Math.sin(angle + Math.PI / 6)
      );
      ctx.closePath();
      ctx.fill();

      if (label) {
        ctx.fillStyle = colors.muted;
        ctx.font = '11px system-ui, sans-serif';
        ctx.textAlign = 'center';
        if (vertical) {
          ctx.save();
          ctx.translate((fromX + toX) / 2 - 10, (fromY + toY) / 2);
          ctx.rotate(-Math.PI / 2);
          ctx.fillText(label, 0, 0);
          ctx.restore();
        } else {
          ctx.fillText(label, (fromX + toX) / 2, (fromY + toY) / 2 - 8);
        }
      }
    };

    // Draw dashed arrow helper
    const drawDashedArrow = (fromX: number, fromY: number, toX: number, toY: number, color: string) => {
      ctx.setLineDash([5, 3]);
      ctx.strokeStyle = color;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(fromX, fromY);
      ctx.lineTo(toX, toY);
      ctx.stroke();
      ctx.setLineDash([]);
      // Arrowhead
      const angle = Math.atan2(toY - fromY, toX - fromX);
      const headLen = 8;
      ctx.fillStyle = color;
      ctx.beginPath();
      ctx.moveTo(toX, toY);
      ctx.lineTo(
        toX - headLen * Math.cos(angle - Math.PI / 6),
        toY - headLen * Math.sin(angle - Math.PI / 6)
      );
      ctx.lineTo(
        toX - headLen * Math.cos(angle + Math.PI / 6),
        toY - headLen * Math.sin(angle + Math.PI / 6)
      );
      ctx.closePath();
      ctx.fill();
    };

    // ===== Boxes =====

    // Scheduler
    drawBox(40, 40, 130, 60, colors.accent, 'Scheduler', '(external)');

    // TEE Public Storage
    drawBox(40, 180, 180, 100, colors.secondary, 'TEE Public Storage', '', true);
    ctx.fillStyle = colors.muted;
    ctx.font = '11px system-ui, sans-serif';
    ctx.textAlign = 'left';
    ctx.fillText('price:wrap.near', 55, 220);
    ctx.fillText('price:aurora', 55, 238);
    ctx.fillText('price:nbtc...', 55, 256);

    // OutLayer Coordinator
    drawBox(285, 120, 120, 60, colors.primary, 'OutLayer', 'Coordinator');

    // TEE Worker
    drawBox(500, 80, 170, 180, colors.tee, 'TEE Worker', 'Intel TDX', true);
    ctx.fillStyle = colors.muted;
    ctx.font = '11px system-ui, sans-serif';
    ctx.textAlign = 'left';
    const steps = [
      'Fetch 10 sources',
      'Aggregate (median)',
      'Store in TEE storage',
      'report_prices',
    ];
    steps.forEach((step, i) => {
      // Highlight step 4 in push color
      if (i === 3) {
        ctx.fillStyle = colors.push;
      } else {
        ctx.fillStyle = colors.muted;
      }
      ctx.fillText(`${i + 1}. ${step}`, 515, 140 + i * 22);
    });

    // price-oracle.near contract (NEW)
    drawBox(490, 310, 190, 50, colors.push, 'price-oracle.near', 'NEAR contract');

    // ===== Arrows =====

    // Scheduler monitors storage
    drawArrow(105, 100, 105, 180, colors.accent);
    ctx.fillStyle = colors.muted;
    ctx.font = '11px system-ui, sans-serif';
    ctx.textAlign = 'left';
    ctx.fillText('monitors', 115, 140);

    // Scheduler -> OutLayer (if stale)
    ctx.fillStyle = colors.muted;
    ctx.font = '10px system-ui, sans-serif';
    ctx.textAlign = 'left';
    ctx.fillText('if stale or', 210, 75);
    ctx.fillText('deviation > 1%', 210, 88);
    drawArrow(170, 70, 280, 140, colors.accent);

    // OutLayer -> TEE Worker
    drawArrow(405, 150, 495, 150, colors.primary, 'execute WASI');

    // On-chain requests -> TEE (from contracts, via OutLayer)
    drawDashedArrow(480, 60, 500, 80, colors.text);
    ctx.fillStyle = colors.text;
    ctx.font = '11px system-ui, sans-serif';
    ctx.textAlign = 'center';
    ctx.fillText('on-chain requests', 440, 45);
    ctx.fillStyle = colors.muted;
    ctx.font = '10px system-ui, sans-serif';
    ctx.fillText('(on-demand)', 440, 58);

    // TEE -> Storage (dashed purple)
    drawDashedArrow(500, 220, 220, 220, colors.tee);
    ctx.fillStyle = colors.muted;
    ctx.font = '11px system-ui, sans-serif';
    ctx.textAlign = 'center';
    ctx.fillText('store prices', 360, 210);

    // TEE -> Contract (dashed red — proactive push, NEW)
    drawDashedArrow(585, 260, 585, 310, colors.push);
    ctx.fillStyle = colors.push;
    ctx.font = '11px system-ui, sans-serif';
    ctx.textAlign = 'left';
    ctx.fillText('report_prices', 595, 280);
    ctx.font = '10px system-ui, sans-serif';
    ctx.fillText('(TEE-signed key)', 595, 293);

    // Legend
    ctx.fillStyle = colors.muted;
    ctx.font = '11px system-ui, sans-serif';
    ctx.textAlign = 'left';
    ctx.fillText('Proactive pushing: prices always fresh in contract (30-60s updates)', 10, height - 15);

    // Legend color indicators
    const legendY = height - 38;
    // On-demand
    ctx.setLineDash([5, 3]);
    ctx.strokeStyle = colors.text;
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(10, legendY);
    ctx.lineTo(35, legendY);
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.fillStyle = colors.muted;
    ctx.font = '10px system-ui, sans-serif';
    ctx.fillText('on-demand', 40, legendY + 3);
    // Proactive push
    ctx.setLineDash([5, 3]);
    ctx.strokeStyle = colors.push;
    ctx.beginPath();
    ctx.moveTo(110, legendY);
    ctx.lineTo(135, legendY);
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.fillStyle = colors.push;
    ctx.fillText('proactive push', 140, legendY + 3);

  }, []);

  return (
    <div className="w-full overflow-x-auto">
      <canvas
        ref={canvasRef}
        className="mx-auto rounded-lg"
        style={{ maxWidth: '100%', height: 'auto' }}
      />
    </div>
  );
}
