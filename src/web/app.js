const checkList = document.getElementById("checkList");
const results = document.getElementById("results");
const form = document.getElementById("checkForm");
const formStatus = document.getElementById("formStatus");
const healthValue = document.getElementById("healthValue");

let checksCache = [];
let selectedId = null;

async function fetchHealth() {
  try {
    const response = await fetch("/health");
    if (!response.ok) {
      throw new Error("Health check failed");
    }
    healthValue.textContent = "Operativo";
    healthValue.classList.remove("down");
  } catch (error) {
    healthValue.textContent = "Sin respuesta";
    healthValue.classList.add("down");
  }
}

async function loadChecks() {
  const response = await fetch("/checks");
  if (!response.ok) {
    throw new Error("No se pudieron cargar los checks");
  }
  checksCache = await response.json();
  renderChecks();
  if (checksCache.length && !selectedId) {
    selectCheck(checksCache[0].id);
  }
}

function renderChecks() {
  checkList.innerHTML = "";
  if (!checksCache.length) {
    checkList.innerHTML = "<li>No hay checks aún.</li>";
    return;
  }

  checksCache.forEach((check) => {
    const item = document.createElement("li");
    item.className = "check-item";
    if (check.id === selectedId) {
      item.classList.add("active");
    }

    const badgeClass = check.last_status === "DOWN" ? "down" : "up";
    const statusLabel = check.last_status ?? "SIN DATOS";

    item.innerHTML = `
      <div>
        <strong>${check.name}</strong>
        <p>${check.url}</p>
      </div>
      <div class="row">
        <span class="badge ${badgeClass}">${statusLabel}</span>
        <span>Intervalo: ${check.interval_seconds}s</span>
      </div>
    `;

    item.addEventListener("click", () => selectCheck(check.id));
    checkList.appendChild(item);
  });
}

async function selectCheck(id) {
  selectedId = id;
  renderChecks();
  await loadResults(id);
}

async function loadResults(id) {
  results.innerHTML = "Cargando...";
  const response = await fetch(`/checks/${id}/results`);
  if (!response.ok) {
    results.innerHTML = "No se pudieron cargar los resultados.";
    return;
  }
  const data = await response.json();
  renderResults(data.slice(0, 6));
}

function renderResults(list) {
  if (!list.length) {
    results.innerHTML = "No hay resultados todavía.";
    return;
  }

  results.innerHTML = "";
  list.forEach((result) => {
    const item = document.createElement("div");
    item.className = "result-item";
    const badgeClass = result.status === "DOWN" ? "down" : "up";

    item.innerHTML = `
      <div class="row">
        <span class="badge ${badgeClass}">${result.status}</span>
        <span>${new Date(result.checked_at).toLocaleString()}</span>
      </div>
      <div>
        <strong>HTTP:</strong> ${result.http_status ?? "-"}
        <strong> Latencia:</strong> ${result.latency_ms ?? "-"} ms
      </div>
      <div>${result.error ? `Error: ${result.error}` : ""}</div>
    `;

    results.appendChild(item);
  });
}

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  formStatus.textContent = "";
  formStatus.classList.remove("error");

  const formData = new FormData(form);
  const payload = {
    name: formData.get("name"),
    url: formData.get("url"),
    interval_seconds: Number(formData.get("interval_seconds")),
    alert_email: formData.get("alert_email") || null,
  };

  try {
    const response = await fetch("/checks", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(payload),
    });

    if (!response.ok) {
      const message = await response.text();
      throw new Error(message || "Error al crear el check");
    }

    form.reset();
    formStatus.textContent = "Check creado correctamente.";
    await loadChecks();
  } catch (error) {
    formStatus.textContent = error.message;
    formStatus.classList.add("error");
  }
});

fetchHealth();
loadChecks();
setInterval(fetchHealth, 15000);
setInterval(loadChecks, 15000);
