// 日期 / 时间 / 配额窗格式化纯函数模块。
// 由 app.js 拆出，在 app.js 之前以 <script defer> 加载，
// 通过共享全局作用域向 app.js 提供下列函数，无需修改调用方。

function formatDateTime(timestampSeconds) {
  if (!timestampSeconds) {
    return "—";
  }

  return formatTimeOnly(new Date(timestampSeconds * 1000));
}

function formatMonthDayTimeValue(value) {
  if (!(value instanceof Date) || Number.isNaN(value.getTime())) {
    return "—";
  }

  return value.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

function formatMonthDayTime(timestampSeconds) {
  if (!timestampSeconds) {
    return "—";
  }

  return formatMonthDayTimeValue(new Date(timestampSeconds * 1000));
}

function firstFiniteNumber(...values) {
  for (const value of values) {
    if (typeof value === "number" && Number.isFinite(value)) {
      return Math.round(value);
    }
    if (typeof value === "string" && value.trim()) {
      const parsed = Number(value);
      if (Number.isFinite(parsed)) {
        return Math.round(parsed);
      }
    }
  }
  return null;
}

function parseDateLikeValue(value) {
  if (typeof value === "string" && value.trim()) {
    const parsed = new Date(value.trim());
    return Number.isNaN(parsed.getTime()) ? null : parsed;
  }

  const timestamp = firstFiniteNumber(value);
  if (timestamp === null) {
    return null;
  }

  const timestampMs = Math.abs(timestamp) >= 1e12 ? timestamp : timestamp * 1000;
  const parsed = new Date(timestampMs);
  return Number.isNaN(parsed.getTime()) ? null : parsed;
}

function formatTimeOnly(value) {
  if (!(value instanceof Date) || Number.isNaN(value.getTime())) {
    return "—";
  }

  return value.toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

function formatDateLikeTime(value) {
  if (typeof value === "string" && value.trim()) {
    const parsed = parseDateLikeValue(value);
    return parsed ? formatTimeOnly(parsed) : value.trim();
  }

  const parsed = parseDateLikeValue(value);
  return parsed ? formatTimeOnly(parsed) : "—";
}

function formatDateLikeMonthDayTime(value) {
  if (typeof value === "string" && value.trim()) {
    const parsed = parseDateLikeValue(value);
    return parsed ? formatMonthDayTimeValue(parsed) : value.trim();
  }

  const parsed = parseDateLikeValue(value);
  return parsed ? formatMonthDayTimeValue(parsed) : "—";
}

function formatElapsedSince(value) {
  const parsed = parseDateLikeValue(value);
  if (!parsed) {
    return {
      label: "—",
      stale: false,
    };
  }

  const diffMs = Math.max(0, Date.now() - parsed.getTime());
  const totalSeconds = Math.floor(diffMs / 1000);
  const totalMinutes = Math.floor(totalSeconds / 60);
  const totalHours = Math.floor(totalMinutes / 60);
  const days = Math.floor(totalHours / 24);
  const hours = totalHours % 24;
  const minutes = totalMinutes % 60;

  let label = "刚刚";
  if (days > 0) {
    label = `${days}d ${hours}h`;
  } else if (totalHours > 0) {
    label = minutes > 0 ? `${totalHours}h ${minutes}m` : `${totalHours}h`;
  } else if (totalMinutes > 0) {
    label = `${totalMinutes}m`;
  } else if (totalSeconds > 0) {
    label = `${totalSeconds}s`;
  }

  return {
    label,
    stale: diffMs > 86400000,
  };
}

function formatRemainingDuration(resetTimeSeconds) {
  const diff = Math.max(0, Math.floor(resetTimeSeconds - Date.now() / 1000));
  if (diff <= 0) {
    return "已重置";
  }

  const days = Math.floor(diff / 86400);
  const hours = Math.floor((diff % 86400) / 3600);
  const minutes = Math.floor((diff % 3600) / 60);

  if (days > 0) {
    return `${days}d ${hours}h`;
  }
  if (hours > 0) {
    return minutes > 0 ? `${hours}h ${minutes}m` : `${hours}h`;
  }
  if (minutes > 0) {
    return `${minutes}m`;
  }
  return "1m";
}

function formatQuotaWindow(percentage, resetTimeSeconds) {
  const hasPercentage = Number.isFinite(percentage);
  const hasResetTime = Number.isFinite(resetTimeSeconds);
  if (!hasPercentage && !hasResetTime) {
    return "—";
  }

  const quotaLabel = hasPercentage ? `${percentage}%` : "";
  if (!hasResetTime) {
    return quotaLabel || "—";
  }

  const remaining = formatRemainingDuration(resetTimeSeconds);
  if (remaining === "已重置") {
    return quotaLabel ? `${quotaLabel} · 已重置` : "已重置";
  }

  const includeAbsolute = resetTimeSeconds - Date.now() / 1000 >= 86400;
  const timeLabel = includeAbsolute ? `${remaining} (${formatMonthDayTime(resetTimeSeconds)})` : remaining;
  return [quotaLabel, timeLabel].filter(Boolean).join(" · ");
}

function textOrDash(value) {
  return typeof value === "string" && value.trim() ? value.trim() : "—";
}
