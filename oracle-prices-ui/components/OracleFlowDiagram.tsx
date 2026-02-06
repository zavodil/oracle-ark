'use client';

import { useEffect, useRef } from 'react';

interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
  label: string;
  sublabel?: string;
  color: string;
  labelTop?: boolean; // Position label at top of box instead of center
}

export default function OracleFlowDiagram() {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // Set canvas size
    const dpr = window.devicePixelRatio || 1;
    const width = 600;
    const height = 350;
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
      text: '#e5e7eb',
      muted: '#6b7280',
    };

    // Draw rounded box
    const drawBox = (box: Box) => {
      ctx.strokeStyle = box.color;
      ctx.lineWidth = 2;
      ctx.fillStyle = `${box.color}15`;

      const r = 8;
      ctx.beginPath();
      ctx.moveTo(box.x + r, box.y);
      ctx.lineTo(box.x + box.width - r, box.y);
      ctx.quadraticCurveTo(box.x + box.width, box.y, box.x + box.width, box.y + r);
      ctx.lineTo(box.x + box.width, box.y + box.height - r);
      ctx.quadraticCurveTo(box.x + box.width, box.y + box.height, box.x + box.width - r, box.y + box.height);
      ctx.lineTo(box.x + r, box.y + box.height);
      ctx.quadraticCurveTo(box.x, box.y + box.height, box.x, box.y + box.height - r);
      ctx.lineTo(box.x, box.y + r);
      ctx.quadraticCurveTo(box.x, box.y, box.x + r, box.y);
      ctx.closePath();
      ctx.fill();
      ctx.stroke();

      ctx.fillStyle = colors.text;
      ctx.font = 'bold 14px system-ui, sans-serif';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';

      // Position label at top or center
      const labelY = box.labelTop
        ? box.y + 22
        : box.y + box.height / 2 - (box.sublabel ? 8 : 0);
      ctx.fillText(box.label, box.x + box.width / 2, labelY);

      if (box.sublabel) {
        ctx.fillStyle = colors.muted;
        ctx.font = '12px system-ui, sans-serif';
        const sublabelY = box.labelTop ? box.y + 38 : box.y + box.height / 2 + 12;
        ctx.fillText(box.sublabel, box.x + box.width / 2, sublabelY);
      }
    };

    // Draw curved arrow with arrowhead
    const drawCurvedArrow = (
      startX: number, startY: number,
      controlX: number, controlY: number,
      endX: number, endY: number,
      color: string,
      label?: string,
      sublabel?: string,
      labelOffset?: { x: number; y: number }
    ) => {
      ctx.strokeStyle = color;
      ctx.lineWidth = 2;
      ctx.setLineDash([]);

      // Draw curve
      ctx.beginPath();
      ctx.moveTo(startX, startY);
      ctx.quadraticCurveTo(controlX, controlY, endX, endY);
      ctx.stroke();

      // Calculate tangent at end point for arrowhead direction
      const t = 0.99;
      const tangentX = 2 * (1 - t) * (controlX - startX) + 2 * t * (endX - controlX);
      const tangentY = 2 * (1 - t) * (controlY - startY) + 2 * t * (endY - controlY);
      const angle = Math.atan2(tangentY, tangentX);

      // Draw arrowhead
      const headLen = 10;
      ctx.fillStyle = color;
      ctx.beginPath();
      ctx.moveTo(endX, endY);
      ctx.lineTo(
        endX - headLen * Math.cos(angle - Math.PI / 6),
        endY - headLen * Math.sin(angle - Math.PI / 6)
      );
      ctx.lineTo(
        endX - headLen * Math.cos(angle + Math.PI / 6),
        endY - headLen * Math.sin(angle + Math.PI / 6)
      );
      ctx.closePath();
      ctx.fill();

      // Draw label
      if (label) {
        const labelX = controlX + (labelOffset?.x || 0);
        const labelY = controlY + (labelOffset?.y || 0);

        ctx.fillStyle = colors.text;
        ctx.font = '12px system-ui, sans-serif';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(label, labelX, labelY);

        if (sublabel) {
          ctx.fillStyle = colors.muted;
          ctx.font = '11px system-ui, sans-serif';
          ctx.fillText(sublabel, labelX, labelY + 14);
        }
      }
    };

    // Draw step number
    const drawStep = (x: number, y: number, num: number) => {
      ctx.fillStyle = colors.accent;
      ctx.beginPath();
      ctx.arc(x, y, 12, 0, Math.PI * 2);
      ctx.fill();

      ctx.fillStyle = '#0f1419';
      ctx.font = 'bold 12px system-ui, sans-serif';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(num.toString(), x, y);
    };

    // Layout: Contract (left) → OutLayer (top-right) → TEE (bottom-right) → back to Contract
    const yourContract: Box = {
      x: 40,
      y: 130,
      width: 140,
      height: 70,
      label: 'Your Contract',
      sublabel: 'DeFi / dApp',
      color: colors.primary,
    };

    const outlayer: Box = {
      x: 350,
      y: 50,
      width: 150,
      height: 60,
      label: 'OutLayer',
      sublabel: 'outlayer.near',
      color: colors.accent,
    };

    const tee: Box = {
      x: 350,
      y: 220,
      width: 180,
      height: 110,
      label: 'TEE Worker',
      sublabel: 'Intel TDX',
      color: colors.tee,
      labelTop: true,
    };

    // Draw boxes
    drawBox(yourContract);
    drawBox(outlayer);
    drawBox(tee);

    // TEE internal details (below the label)
    ctx.fillStyle = colors.muted;
    ctx.font = '11px system-ui, sans-serif';
    ctx.textAlign = 'left';
    const teeSteps = ['• Fetch any API', '• Run WASI code', '• Return result'];
    teeSteps.forEach((step, i) => {
      ctx.fillText(step, tee.x + 15, tee.y + 55 + i * 16);
    });

    // Arrow 1: Contract → OutLayer (up and right)
    drawCurvedArrow(
      yourContract.x + yourContract.width - 10, yourContract.y,  // start: right side of contract, near top
      250, 30,                                                     // control point: above and between
      outlayer.x, outlayer.y + outlayer.height / 2,               // end: left side of outlayer
      colors.accent,
      'request_execution()',
      'yield + 0.01 NEAR',
      { x: -20, y: 20 }
    );
    drawStep(230, 25, 1);

    // Arrow 2: OutLayer → TEE (curved right, like right side of circle)
    drawCurvedArrow(
      outlayer.x + outlayer.width / 2, outlayer.y + outlayer.height,  // start: bottom center of outlayer
      480, 165,                                                        // control point: right side for rightward bulge
      tee.x + tee.width / 2, tee.y,                                   // end: top center of TEE
      colors.tee,
      'execute WASI',
      undefined,
      { x: 20, y: 0 }
    );
    drawStep(440, 165, 2);

    // Arrow 3: TEE → Contract (up and left)
    drawCurvedArrow(
      tee.x, tee.y + tee.height / 2,                              // start: left side of TEE
      200, 320,                                                    // control point: below and between
      yourContract.x + yourContract.width - 20, yourContract.y + yourContract.height,  // end: right side of contract, near bottom
      colors.secondary,
      'resume with result',
      'callback to contract',
      { x: 30, y: -25 }
    );
    drawStep(160, 300, 3);

    // Legend
    ctx.fillStyle = colors.muted;
    ctx.font = '11px system-ui, sans-serif';
    ctx.textAlign = 'left';
    ctx.fillText('No intermediary contracts - your contract talks to TEE directly', 10, height - 12);

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
