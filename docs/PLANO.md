# FitLedger — Plano de desenvolvimento

Aplicativo desktop para Windows, local e pessoal, que funciona como diário esportivo de corrida de rua, esteira, musculação e tênis.
Repositório público `FitLedger`. Segue o guia `RECOMENDACOES-EXE-RUST.md`.

---

## 1. O que os arquivos de teste mostram

Os 5 arquivos em `fits/` vêm de um **Amazfit Active 2 (Round)**, exportados pelo Zepp.

| Data  | sport / sub_sport   | Duração | Distância | FC méd/máx | Dados por segundo |
|-------|---------------------|---------|-----------|------------|-------------------|
| 03/05 | running / street    | 30:24   | 5,03 km   | 179 / 196  | GPS, altitude, FC, cadência, velocidade, distância, potência, passada, oscilação/contato |
| 09/08 | running / street    | 29:36   | 5,04 km   | 174 / 189  | igual |
| 17/08 | running / treadmill | 30:05   | 5,01 km   | 170 / 184  | **sem GPS e sem altitude**; distância estimada pelo pulso |
| 26/08 | running / street    | 28:53   | 5,01 km   | 168 / 182  | igual |
| 08/10 | tennis              | 76:09   | —         | 159 / 181  | **só FC** |

O que isso muda no plano:

- **Voltas** (`lap`) vêm por km. O `session` já traz o tempo em cada zona de FC (`time_in_hr_zone`), training effect e calorias.
- **Não há temperatura** nesses arquivos. O campo existe no app, mas fica vazio.
- **Há campos de desenvolvedor** (`developer_data_id` e `field_description`). Guardamos o arquivo original inteiro, então nada se perde, mesmo que ainda não os usemos.
- **Distância na esteira** é estimada pelo relógio. Por isso o app permite corrigir a distância e a inclinação à mão, e records feitos na esteira ficam separados (ver item 39).
- **Os arquivos contêm GPS** perto de casa. **Nunca vão para o repositório público**: `fits/` vai no `.gitignore`, e os testes usam arquivos FIT sintéticos (ver §8).

---

## 2. Decisões técnicas

| Tema | Escolha | Motivo |
|---|---|---|
| Linguagem | Rust (stable, hoje 1.97) | Um único .exe, sem runtime. É o mesmo caminho do projeto VPN. |
| Interface | `eframe`/`egui` com o desenho **padrão (glow/OpenGL)** | Não precisa de RDP, então o .exe fica menor e usa menos memória. |
| Gráficos | `egui_plot` | Hover com valores, zoom, eixos e cursor ligados entre os gráficos da mesma atividade. |
| Tabelas e datas | `egui_extras` (TableBuilder e DatePicker) | Tabelas grandes com rolagem virtual e ordenação. |
| Banco | SQLite com `rusqlite` (feature `bundled`) | Um arquivo só, transações, aguenta anos de dados. |
| Leitura de FIT | **Decodificador próprio** (`src/fit/`) | Sem dependência extra; confere o CRC, aceita arquivos encadeados e timestamps comprimidos, e nunca entra em pânico com arquivo corrompido. Lê só os campos que usamos; o arquivo bruto fica guardado. |
| Deduplicação | SHA-256 pelo BCrypt do Windows + chave da atividade | Ver §4. |
| Diálogos de arquivo | `rfd` | Para "Importar arquivos FIT", "Criar backup" etc. |
| Datas | `chrono` | Guardamos em UTC e mostramos no fuso local. |
| Exportação | `serde`/`serde_json` e `csv` | — |
| Windows | `windows-sys` (instância única, WinHTTP, BCrypt, WinVerifyTrust), `embed-resource` (ícone, versão, manifesto `asInvoker`) | — |
| Atualização automática | Igual ao VPN: consulta `releases/latest` do GitHub uma vez por dia (pode desligar), baixa o `FitLedger.exe`, confere o SHA-256 publicado pelo GitHub (e o assinante, quando houver assinatura), troca o .exe e reabre | Pedido em 09/10/2026. É o único acesso à rede. |

**Onde ficam os arquivos:**

- `<pasta do exe>\fits\` guarda os FIT de entrada. O app só **lê** essa pasta e a cria se ela não existir.
- `<pasta do exe>\data\fitledger.db` guarda todos os dados do app (**modo portátil**, por sua escolha), junto com backups automáticos de antes das migrações.
  - O app confere ao abrir se consegue gravar na pasta. Se não conseguir (por exemplo, se estiver em `Program Files`), mostra uma mensagem clara e não abre.
  - Na primeira vez que o banco for criado, um aviso explica que apagar a pasta do app apaga o histórico, e recomenda usar "Criar backup".
- Nenhum dado de treino sai do computador. **Sem telemetria.** O mapa do percurso é desenhado como uma linha, **sem baixar mapas**. O único acesso à rede é a verificação de atualização (desligável).

**Idioma:** a interface sai em **pt-BR e inglês desde o início**.
- Todos os textos ficam em `i18n.rs`, como uma chave com os dois idiomas.
- Um teste garante que nenhuma chave fica sem tradução.
- O idioma é escolhido nas Configurações e no assistente de primeiro uso. O padrão segue o idioma do Windows.
- Números e datas seguem o idioma: `6:00/km` e `10,0 km/h` em pt-BR, `10.0 km/h` em inglês.

O README sai em inglês, com link para o `README.pt-BR.md`.

---

## 3. Arquitetura

```
fitledger/
├─ Cargo.toml / Cargo.lock / build.rs / app.rc / app.manifest
├─ src/
│  ├─ main.rs              # #![windows_subsystem="windows"], instância única, abre a janela
│  ├─ app.rs               # estado da UI, navegação, tema
│  ├─ paths.rs             # pasta do exe, /fits, %APPDATA%
│  ├─ db/                  # conexão, migrações versionadas, repositórios
│  ├─ fit/                 # decodificação → modelo interno (Activity, Lap, Sample, Event)
│  ├─ import/              # varredura, hash, deduplicação, relatório de erros
│  ├─ domain/              # tipos: Modality, Units, Pace, Zones, Plan, Goal...
│  ├─ analysis/            # funções puras, sem UI e sem banco
│  │   ├─ quality.rs       # marca leituras ruins
│  │   ├─ splits.rs  zones.rs  drift.rs  pace_hr.rs  best_efforts.rs
│  │   ├─ load.rs  recovery.rs  comparability.rs  insights.rs  summary.rs
│  ├─ ui/                  # uma tela por arquivo, com componentes reutilizáveis (cards, gráficos, filtros)
│  ├─ backup.rs  export.rs
│  └─ i18n.rs
└─ tests/                  # testes de integração com FIT sintético
```

**Princípio:** os cálculos em `analysis/` são **funções puras** que recebem amostras e configurações e devolvem métricas. Isso deixa os testes fáceis e faz o "Recalcular métricas" ser só rodar tudo de novo.

**Desempenho:** consultas pesadas, como importação e recálculo, rodam numa thread separada. A UI recebe progresso por um canal e nunca trava.

---

## 4. Modelo de dados (SQLite)

Há três camadas: **bruto → normalizado → derivado**. Só a última é apagada e refeita.

**Bruto (nunca muda)**
- `fit_file`: id, sha256 (único), nome original, tamanho, data de importação, **conteúdo bruto (BLOB)**, status (ok/erro), mensagem de erro.
  Guardar o bruto custa ~50 KB por atividade (alguns MB por ano). Em troca, o backup fica completo e o reprocessamento não depende da pasta `/fits`.

**Normalizado (vem do FIT; o usuário só sobrescreve campos de edição)**
- `activity`: id, fit_file_id (vazio se for manual), início (UTC), fuso, modalidade, sub-tipo FIT original, tempo total, tempo em movimento, distância, calorias, dispositivo, FC méd/máx, cadência méd/máx, velocidade méd/máx, potência méd/máx, ganho/perda de elevação, temperatura, training effect, origem (`fit`/`manual`).
- `activity_override`: correções do usuário (modalidade, distância da esteira, inclinação, título), **separadas** do valor original.
- `lap`, `event` (pausas e timer), `sample` (timestamp, lat, lon, altitude, FC, velocidade, distância, cadência, potência, temperatura, passada etc.).
  São ~1 800 amostras por corrida, o que dá alguns milhões de linhas em anos. O SQLite com índice por `activity_id` lida bem com isso.
- `activity_note`: texto, RPE 1–10, sono, fadiga, dor muscular, peso corporal.

**Usuário**
- `settings` (chave/valor JSON), `hr_zone_config` (com histórico de vigência, porque as zonas mudam com o tempo).
- `exercise`, `strength_session`, `strength_set` (exercício, série, reps, peso, RPE, descanso, notas).
- `plan`, `planned_workout` (data, modalidade, tipo de treino, alvos, `activity_id` vinculada), `workout_step` (árvore de blocos com repetição).
- `reference_test` (definição: distância, pace, velocidade e inclinação) e `reference_test_run` (activity_id).
- `goal` (tipo, parâmetros JSON, período) e `objective` (com submetas ligadas a `goal`).

**Derivado (cache, apagado e refeito pelo "Recalcular métricas")**
- `sample_flag` (amostra e motivo), `activity_metrics` (zonas, drift, carga, eficiência, comparabilidade), `split`, `best_effort`, `pace_hr_point`, `insight` (texto + dados de evidência em JSON).
- `metrics_version`: quando o algoritmo muda, o app recalcula sozinho na abertura.

**Migrações** versionadas por `PRAGMA user_version`. Antes de cada migração, o app faz um backup automático do banco.

---

## 5. Regras de negócio

**Importação**
1. Ao abrir, o app procura `*.fit` em `<exe>\fits` (cria a pasta se faltar) e calcula o SHA-256 de cada arquivo.
2. Se o hash já existe, pula.
3. Se o arquivo é novo, aplica a segunda checagem: **mesmo dispositivo (serial) + mesmo `time_created`**, ou mesmo início ±60 s e mesma duração. Isso pega o mesmo treino reexportado com outro nome. Nesse caso, avisa e não duplica.
4. Cada arquivo roda numa transação própria. Se um falha, o erro vai para a lista e os outros continuam.
5. Ao final, mostra "N novas, M já existentes, K com erro (ver detalhes)".
6. O arquivo original é aberto só para leitura.

**Modalidade (mapeamento inicial, editável)**
- running + treadmill → **Esteira**; running + outros → **Rua**
- tennis → **Tênis**
- training/strength_training → **Musculação**
- resto → **Outras**

**Qualidade dos dados (item 38)**
Marca, sem apagar: FC = 0 ou fora de 30–230; saltos de FC acima de X bpm/s; velocidade acima de 25 km/h a pé; salto de GPS com velocidade implícita impossível; cadência isolada fora da mediana móvel. As métricas ignoram as amostras marcadas, e a tela de detalhe mostra quantas foram ignoradas.

**Métricas principais**
- **Tempo em movimento:** velocidade acima do limiar e intervalos sem pausa.
- **Splits:** por distância acumulada, interpolando o tempo na fronteira (500 m, 1 km, milha ou distância personalizada).
- **Zonas:** por FC máx, Karvonen (reserva) ou bpm personalizado. Calcula o tempo e o % por zona e mostra a faixa de bpm.
- **Cardiac drift:** compara FC média e pace entre a 1ª e a 2ª metade, e calcula o **desacoplamento Pa:HR** (variação da razão velocidade/FC). Opcionalmente descarta os primeiros N minutos de aquecimento.
- **Pace × FC:** guarda dois níveis.
  - Por atividade: pace e FC médios.
  - Por trechos estáveis: janelas de 60 s com pace estável, agrupadas em faixas de 15 s/km.
  Gráficos: FC por data dentro da mesma faixa de pace, e **índice de eficiência** (m/min ÷ bpm) ao longo do tempo.
- **Melhores marcas:** janela deslizante sobre a distância (400 m, 1 km, milha, 3 km, 5 km, 10 km). A esteira tem records separados.
- **Carga:** TRIMP por FC (Banister, com zona); sem FC, usa sRPE (RPE × minutos). Mostra carga diária, semanal, aguda de 7 dias, crônica de 28 dias e a razão entre elas. O tênis entra com a FC real dele.
- **Recuperação:** classifica em "recuperado", "carga moderada" ou "carga alta" pela razão aguda/crônica, dias seguidos de treino e intensidade das últimas 48 h. Sempre com o aviso "estimativa baseada nos treinos, não é avaliação médica".
- **Comparabilidade (item 39):** cada atividade ganha etiquetas (esteira/rua, plano/ondulado por m de subida por km, faixa de distância, duração, temperatura). Ao comparar atividades com etiquetas diferentes, aparece um aviso visível.
- **Insights:** regras simples, cada uma com limiar mínimo de dados. Cada insight guarda e mostra a evidência (atividades, datas e valores).

---

## 6. Telas

Menu lateral (item 35):

- **Dashboard**
- **Atividades**: Todas, Rua, Esteira, Tênis, Musculação
- **Calendário**
- **Planos**
- **Metas**
- **Evolução**
- **Records**
- **Análises**, com abas: Pace × FC, Cardiac drift, Testes de referência, Comparar, Zonas de FC e Conversor pace ↔ km/h
- **Insights**
- **Configurações**: perfil, FC e zonas, unidades, tema, backup, exportação e recalcular

O **primeiro uso** abre um assistente: dados opcionais, depois "Encontramos N atividades", depois **Importar**.

Na **tela da atividade**, ficam no topo os cards de resumo e, embaixo, abas:

- **Gráficos:** FC, pace, velocidade, cadência, altitude e potência, todos com cursor ligado. Dá para alternar o eixo X entre tempo e distância.
- **Splits**
- **Zonas**
- **Drift**
- **Voltas**
- **Mapa** (só a linha do percurso)
- **Notas**
- **Qualidade dos dados**

---

## 7. Etapas (na ordem de prioridade do item 41)

Cada etapa termina com o app abrindo, testes passando e um commit.

Estado em 09/10/2026: **etapas 0 a 13 implementadas**; da 14, o pipeline, o README, as capturas e as licenças estão prontos, e falta publicar o repositório e a primeira versão.

| # | Etapa | Itens do pedido | Entrega verificável |
|---|---|---|---|
| 0 | **Fundação** | — | `.gitignore` (inclui `fits/`, `data/`, `*.fit`, `target/`, `*.db`), GPL-3.0, i18n pt-BR/en, Cargo, janela egui com menu lateral, tema claro/escuro, SQLite + migrações, instância única, CI (test, clippy, fmt). |
| 1 | **Importação FIT** | 1, 2, 34 | Os 5 arquivos importados; reimportar não duplica; um FIT corrompido mostra erro sem parar os outros; assistente de primeiro uso. |
| 2 | **Histórico** | 4, 35 | Tabela com ordenação, busca e filtros (período, modalidade, distância, FC). |
| 3 | **Detalhe + gráficos + splits** | 5, 6, 12 | Gráficos com hover; splits de 500 m / 1 km / milha / personalizado; ajuste manual de inclinação e distância na esteira. |
| 4 | **FC e qualidade** | 7, 38 | Configuração de zonas; tempo e % por zona; marcação de leituras ruins. |
| 5 | **Pace × FC, drift e testes** | 8, 9, 10, 11 | Gráfico 179 → 174 → 170 → 168 com os arquivos reais; desacoplamento; "Teste de referência" com FC por km; conversor. |
| 6 | **Comparação** | 23, 39 | 2 ou mais atividades lado a lado, gráficos sobrepostos por distância, avisos de comparabilidade. |
| 7 | **Planos e calendário** | 13, 14, 15, 16, 29 | Planos, tipos de treino, editor de intervalados com blocos repetidos, calendário semanal e mensal com planejado e realizado, vínculo, visão do dia. |
| 8 | **Metas e objetivos** | 17, 18 | 7 tipos de meta com barra de progresso; objetivos com submetas. |
| 9 | **Musculação, manuais e notas** | 19, 26, 30 | Exercícios, séries, histórico por exercício, volume; cadastro manual; notas com RPE, sono etc. |
| 10 | **Tênis, carga e recuperação** | 20, 21, 22 | Área de tênis; carga diária, semanal, 7 dias e 28 dias; avisos; recuperação. |
| 11 | **Evolução, records, resumo e dashboard completo** | 3, 24, 25, 27 | Filtros de 7 dias a "tudo"; records automáticos; resumo semanal com comparação. |
| 12 | **Insights** | 28 | Regras com evidência visível. |
| 13 | **Backup, exportação e recálculo** | 31, 32, 37 | Backup com `VACUUM INTO` (um único arquivo `.fitledger`); restaurar com validação e cópia de segurança do banco atual; CSV e JSON; "Recalcular métricas". |
| 14 | **Polimento e release** | 36 | Ícone, README (en + pt-BR), prints com dados falsos, `release.yml` (draft, SHA-256, atestado de origem), Dependabot, `cargo audit` e `cargo deny`. |

O dashboard aparece já na etapa 2 com o que existir, e ganha cards a cada etapa.

---

## 8. Testes

- **Unitários** em `analysis/`: splits com interpolação, zonas, drift, melhores marcas, TRIMP, conversão pace ↔ km/h (os 6 exemplos do pedido viram testes), qualidade dos dados.
- **FIT sintético:** um gerador pequeno, usado só nos testes, escreve arquivos FIT válidos com dados inventados (corrida com GPS falso, esteira, tênis só com FC, arquivo truncado, CRC errado). **Nada de dados reais no repositório.**
- **Arquivos reais** só localmente. Um teste marcado `#[ignore]` lê a pasta indicada em `FITLEDGER_REAL_FITS` e confere os valores da tabela do §1.
- **Banco:** migração de um banco vazio, de um banco na versão anterior, e backup → restauração → mesmos dados (item 40.25).
- **UI:** verificação manual a cada etapa, abrindo o .exe com os 5 arquivos reais.

---

## 9. Repositório e privacidade (do guia)

- O `.gitignore` é o primeiro arquivo, antes de qualquer `git add`.
- Commits com o e-mail *noreply* do GitHub e **sem coautor**, como pede o guia.
- `Cargo.lock` versionado; builds com `--locked`; perfil de release otimizado para tamanho; `--remap-path-prefix` para não gravar caminhos da sua máquina no .exe.
- CI com permissões mínimas, actions fixadas por hash, runner `windows-2022`.
- Release por tag `vX.Y.Z`, sai como **rascunho**.
- `LICENSE`, `SECURITY.md` e uma seção "Privacy" no README dizendo que tudo fica local, sem rede e sem telemetria.

---

## 10. Decisões tomadas (09/10/2026)

1. **Licença:** GPL-3.0. Todas as dependências precisam ter licença compatível, e o `cargo deny` confere isso.
2. **Banco:** portátil, em `<exe>\data\`. O `.gitignore` inclui `data/`.
3. **Idioma:** pt-BR e inglês desde o início.
4. **Mapa:** só a linha do percurso, offline. O app não acessa a rede.
