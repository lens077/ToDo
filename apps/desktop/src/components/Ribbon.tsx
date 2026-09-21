import * as React from 'react';

/**
 * The one coloured object in the interface: a silk ribbon of teal, lilac and
 * amber light rendered by a fragment shader. It reacts to the pointer (the
 * crest bends a few pixels toward the cursor) and otherwise holds still, so it
 * reads as material rather than as motion. Without WebGL, or under reduced
 * motion, a static frame is drawn once and the pointer is ignored.
 */

const VERTEX = `
attribute vec2 a_pos;
void main() { gl_Position = vec4(a_pos, 0.0, 1.0); }
`;

const FRAGMENT = `
precision mediump float;
uniform vec2 u_res;
uniform vec2 u_pointer;   // 0..1 in canvas space, y up; (-1,-1) when absent
uniform float u_bend;     // 0..1 eased pointer influence
uniform float u_glow;     // extra luminance on dark grounds
uniform float u_phase;    // stable fold geometry across mounts

vec3 teal = vec3(0.498, 0.847, 0.816);
vec3 lilac = vec3(0.788, 0.722, 0.941);
vec3 amber = vec3(0.961, 0.647, 0.141);

void main() {
  vec2 uv = gl_FragCoord.xy / u_res;
  float x = uv.x;
  float aspect = u_res.x / max(u_res.y, 1.0);

  // The ribbon's centre line: one long S across the width, folded once.
  float yc = 0.52 + 0.20 * sin((x * 1.9 + u_phase) * 3.14159) - 0.06 * sin((x * 5.3 + u_phase) * 3.14159);
  if (u_pointer.x >= 0.0) {
    float dx = (x - u_pointer.x) * aspect;
    float pull = exp(-dx * dx * 6.0) * (u_pointer.y - yc);
    yc += pull * 0.18 * u_bend;
  }

  // Width breathes along the length so the fold reads as a twist.
  float halfw = 0.115 + 0.055 * sin((x * 3.7 + 0.4 + u_phase) * 3.14159);
  float d = abs(uv.y - yc);
  float edge = 0.012;
  float body = 1.0 - smoothstep(halfw - edge, halfw + edge, d);
  float glow = exp(-(d * d) / (halfw * halfw * 0.9)) * 0.35;

  // A satin highlight runs slightly above the centre line.
  float sheen = exp(-pow((uv.y - (yc + halfw * 0.35)) / (halfw * 0.5), 2.0)) * 0.22;
  float shade = 0.82 + 0.18 * cos((x * 3.7 + 1.2 + u_phase) * 3.14159);

  vec3 col = mix(teal, lilac, smoothstep(0.05, 0.55, x));
  col = mix(col, amber, smoothstep(0.5, 1.0, x));
  col = col * shade + sheen;
  col += u_glow * 0.12;

  // Fade on every edge so the canvas rectangle never prints on a dark ground.
  float fade = smoothstep(0.0, 0.10, x) * (1.0 - smoothstep(0.86, 1.0, x))
    * smoothstep(0.0, 0.12, uv.y) * (1.0 - smoothstep(0.88, 1.0, uv.y));
  float alpha = clamp(body * 0.78 + glow, 0.0, 1.0) * fade;
  gl_FragColor = vec4(col * alpha, alpha);
}
`;

interface RibbonProps {
  className?: string;
  /** Brightness lift for dark grounds; the component reads the scheme itself when omitted. */
  glow?: number;
}

function compile(gl: WebGLRenderingContext, type: number, source: string): WebGLShader | null {
  const shader = gl.createShader(type);
  if (!shader) return null;
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    gl.deleteShader(shader);
    return null;
  }
  return shader;
}

export function Ribbon({ className, glow }: RibbonProps) {
  const ref = React.useRef<HTMLCanvasElement>(null);
  const [fallback, setFallback] = React.useState(false);

  React.useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const gl = canvas.getContext('webgl', { alpha: true, antialias: true, premultipliedAlpha: true, powerPreference: 'low-power' });
    if (!gl) {
      setFallback(true);
      return;
    }
    const vs = compile(gl, gl.VERTEX_SHADER, VERTEX);
    const fs = compile(gl, gl.FRAGMENT_SHADER, FRAGMENT);
    const program = gl.createProgram();
    const release = () => {
      if (program) gl.deleteProgram(program);
      if (vs) gl.deleteShader(vs);
      if (fs) gl.deleteShader(fs);
    };
    if (!vs || !fs || !program) {
      release();
      setFallback(true);
      return;
    }
    gl.attachShader(program, vs);
    gl.attachShader(program, fs);
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      release();
      setFallback(true);
      return;
    }
    gl.useProgram(program);
    const buffer = gl.createBuffer();
    if (!buffer) {
      release();
      setFallback(true);
      return;
    }
    setFallback(false);
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    const position = gl.getAttribLocation(program, 'a_pos');
    gl.enableVertexAttribArray(position);
    gl.vertexAttribPointer(position, 2, gl.FLOAT, false, 0, 0);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);

    const uRes = gl.getUniformLocation(program, 'u_res');
    const uPointer = gl.getUniformLocation(program, 'u_pointer');
    const uBend = gl.getUniformLocation(program, 'u_bend');
    const uGlow = gl.getUniformLocation(program, 'u_glow');
    const uPhase = gl.getUniformLocation(program, 'u_phase');

    const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
    const darkScheme = window.matchMedia('(prefers-color-scheme: dark)');
    const phase = 0.3;

    let pointer = { x: -1, y: -1 };
    let target = { x: -1, y: -1 };
    let bend = 0;
    let bendTarget = 0;
    let frame = 0;
    let disposed = false;

    const resize = () => {
      const rect = canvas.getBoundingClientRect();
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      const width = Math.max(1, Math.round(rect.width * dpr));
      const height = Math.max(1, Math.round(rect.height * dpr));
      if (canvas.width !== width || canvas.height !== height) {
        canvas.width = width;
        canvas.height = height;
      }
      gl.viewport(0, 0, width, height);
    };

    const draw = () => {
      resize();
      gl.uniform2f(uRes, canvas.width, canvas.height);
      gl.uniform2f(uPointer, pointer.x, pointer.y);
      gl.uniform1f(uBend, bend);
      gl.uniform1f(uGlow, glow ?? (darkScheme.matches ? 1 : 0));
      gl.uniform1f(uPhase, phase);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    };

    // Exponential ease toward the pointer; the loop stops once settled so an
    // idle ribbon costs nothing.
    const tick = () => {
      frame = 0;
      if (disposed) return;
      const k = 0.16;
      if (target.x >= 0) {
        pointer = pointer.x < 0 ? { ...target } : { x: pointer.x + (target.x - pointer.x) * k, y: pointer.y + (target.y - pointer.y) * k };
      }
      bend += (bendTarget - bend) * k;
      draw();
      const settled = Math.abs(bendTarget - bend) < 0.004
        && (target.x < 0 || (Math.abs(target.x - pointer.x) < 0.002 && Math.abs(target.y - pointer.y) < 0.002));
      if (!settled) frame = requestAnimationFrame(tick);
    };
    const schedule = () => {
      if (!disposed && !frame) frame = requestAnimationFrame(tick);
    };

    const onMove = (event: PointerEvent) => {
      if (reduceMotion.matches) return;
      const rect = canvas.getBoundingClientRect();
      const margin = 160;
      const inside = event.clientX >= rect.left - margin && event.clientX <= rect.right + margin
        && event.clientY >= rect.top - margin && event.clientY <= rect.bottom + margin;
      if (!inside) {
        if (bendTarget !== 0) {
          bendTarget = 0;
          schedule();
        }
        return;
      }
      target = {
        x: (event.clientX - rect.left) / Math.max(rect.width, 1),
        y: 1 - (event.clientY - rect.top) / Math.max(rect.height, 1),
      };
      bendTarget = 1;
      schedule();
    };
    const onLeave = () => {
      bendTarget = 0;
      schedule();
    };

    const observer = new ResizeObserver(() => schedule());
    observer.observe(canvas);
    const onScheme = () => schedule();
    darkScheme.addEventListener('change', onScheme);
    const onMotion = () => {
      if (reduceMotion.matches) {
        pointer = target = { x: -1, y: -1 };
        bend = bendTarget = 0;
      }
      schedule();
    };
    const onContextLost = (event: Event) => {
      event.preventDefault();
      disposed = true;
      if (frame) cancelAnimationFrame(frame);
      observer.disconnect();
      setFallback(true);
    };
    reduceMotion.addEventListener('change', onMotion);
    window.addEventListener('pointermove', onMove, { passive: true });
    document.addEventListener('pointerleave', onLeave);
    canvas.addEventListener('webglcontextlost', onContextLost);
    draw();

    return () => {
      disposed = true;
      if (frame) cancelAnimationFrame(frame);
      observer.disconnect();
      darkScheme.removeEventListener('change', onScheme);
      window.removeEventListener('pointermove', onMove);
      document.removeEventListener('pointerleave', onLeave);
      reduceMotion.removeEventListener('change', onMotion);
      canvas.removeEventListener('webglcontextlost', onContextLost);
      release();
      gl.deleteBuffer(buffer);
    };
  }, [glow]);

  return (
    <canvas
      ref={ref}
      className={`ribbon${fallback ? ' ribbon--fallback' : ''}${className ? ` ${className}` : ''}`}
      aria-hidden="true"
    />
  );
}
