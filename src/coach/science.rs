//! The evidence behind the coach, shown in the app ("Scientific basis").
//! Each item says what FitLedger does with it, and whether it comes from
//! research or is a conservative rule of thumb chosen by the app.

pub struct Basis {
    pub topic_pt: &'static str,
    pub topic_en: &'static str,
    pub use_pt: &'static str,
    pub use_en: &'static str,
    /// Empty for the app's own rules of thumb.
    pub reference: &'static str,
}

impl Basis {
    pub fn topic(&self) -> &'static str {
        if crate::i18n::pt() {
            self.topic_pt
        } else {
            self.topic_en
        }
    }

    pub fn usage(&self) -> &'static str {
        if crate::i18n::pt() { self.use_pt } else { self.use_en }
    }
}

pub const BASIS: &[Basis] = &[
    Basis {
        topic_pt: "Zonas pela frequência cardíaca de reserva (Karvonen)",
        topic_en: "Zones from heart-rate reserve (Karvonen)",
        use_pt: "As zonas são % da FC de reserva (máxima − repouso) somada à FC de repouso. O topo da zona 2 é o limite das corridas leves.",
        use_en: "Zones are a % of heart-rate reserve (max − rest) added to resting HR. The top of zone 2 is the cap of easy runs.",
        reference: "Karvonen MJ, Kentala E, Mustala O. The effects of training on heart rate: a longitudinal study. Ann Med Exp Biol Fenn. 1957;35(3):307–315.",
    },
    Basis {
        topic_pt: "FC máxima estimada pela idade",
        topic_en: "Age-predicted maximum heart rate",
        use_pt: "Sem FC máxima informada, usa 208 − 0,7 × idade (a maior FC registrada vale se for maior).",
        use_en: "Without a known max HR, uses 208 − 0.7 × age (the highest recorded HR counts if it is higher).",
        reference: "Tanaka H, Monahan KD, Seals DR. Age-predicted maximal heart rate revisited. J Am Coll Cardiol. 2001;37(1):153–156.",
    },
    Basis {
        topic_pt: "Maior parte do treino em baixa intensidade",
        topic_en: "Most training at low intensity",
        use_pt: "A base é só de corridas leves e, depois, no máximo um treino mais forte a cada quatro (intervalado ou ritmo moderado só para quem corre 2 ou mais vezes por semana): o resto continua leve.",
        use_en: "The base is easy running only, and afterwards at most one harder session in four (intervals or tempo only when running 2 or more times a week): the rest stays easy.",
        reference: "Seiler KS, Kjerland GØ. Quantifying training intensity distribution in elite endurance athletes: is there evidence for an \"optimal\" distribution? Scand J Med Sci Sports. 2006;16(1):49–56.",
    },
    Basis {
        topic_pt: "FC submáxima como medida de evolução",
        topic_en: "Submaximal heart rate as a progress marker",
        use_pt: "Com o treino, a FC no mesmo ritmo cai. O app mede isso nos testes de evolução (mesmo pace, de preferência na esteira) e em todas as corridas, ajustadas pela relação FC × velocidade.",
        use_en: "With training, HR at the same pace drops. The app measures it in progress checks (same pace, ideally on the treadmill) and in every run, adjusted by the HR × speed relation.",
        reference: "Buchheit M. Monitoring training status with HR measures: do all roads lead to Rome? Front Physiol. 2014;5:73.",
    },
    Basis {
        topic_pt: "Deriva cardiovascular",
        topic_en: "Cardiovascular drift",
        use_pt: "Compara FC e ritmo entre as metades da corrida (desacoplamento). Deriva alta no longão segura o aumento do longão.",
        use_en: "Compares HR and pace between the halves of a run (decoupling). High drift on the long run holds back its growth.",
        reference: "Coyle EF, González-Alonso J. Cardiovascular drift during prolonged exercise: new perspectives. Exerc Sport Sci Rev. 2001;29(2):88–92.",
    },
    Basis {
        topic_pt: "Carga de treino (TRIMP)",
        topic_en: "Training load (TRIMP)",
        use_pt: "A carga de cada corrida combina duração e intensidade pela FC, com os coeficientes de Banister para homens ou mulheres (o sexo é opcional e só afeta a carga).",
        use_en: "Each run's load combines duration and heart-rate intensity, with Banister's coefficients for men or women (sex is optional and only affects load).",
        reference: "Banister EW. Modeling elite athletic performance. In: MacDougall JD, Wenger HA, Green HJ, eds. Physiological Testing of the High-Performance Athlete. Human Kinetics; 1991:403–424.",
    },
    Basis {
        topic_pt: "Previsão de tempo para outra distância",
        topic_en: "Predicting time over another distance",
        use_pt: "Tempo previsto = tempo conhecido × (distância da meta ÷ distância conhecida)^1,06.",
        use_en: "Predicted time = known time × (goal distance ÷ known distance)^1.06.",
        reference: "Riegel PS. Athletic records and human endurance. Am Sci. 1981;69(3):285–290.",
    },
    Basis {
        topic_pt: "Progressão gradual do volume",
        topic_en: "Gradual volume progression",
        use_pt: "Iniciantes que aumentam muito a distância de uma vez se lesionam mais. O app parte do que você realmente corre e aumenta a corrida leve até 5% e o longão até 10% de cada vez; se os últimos 7 dias passarem muito do seu normal, o próximo treino fica mais leve. Esses números são escolhas conservadoras: um estudo não mostrou que limitar a 10% reduz lesões em relação a um programa padrão.",
        use_en: "Novice runners who increase distance a lot at once get injured more. The app starts from what you actually run and grows the easy run up to 5% and the long run up to 10% at a time; if the last 7 days go well above your usual, the next session is lighter. These numbers are conservative choices: one study found that capping at 10% did not reduce injuries compared with a standard program.",
        reference: "Nielsen RØ et al. Excessive progression in weekly running distance and risk of running-related injuries. J Orthop Sports Phys Ther. 2014;44(10):739–747. · Buist I et al. No effect of a graded training program on the number of running-related injuries in novice runners: a randomized controlled trial. Am J Sports Med. 2008;36(1):33–39.",
    },
    Basis {
        topic_pt: "Periodização e recuperação",
        topic_en: "Periodization and recovery",
        use_pt: "Fases (base, desenvolvimento, polimento), um ciclo mais leve a cada três ciclos de quatro treinos, um dia de descanso depois de um treino forte e uma corrida leve antes da tentativa da meta.",
        use_en: "Phases (base, build, taper), a lighter cycle every third cycle of four sessions, a rest day after a hard session and an easy run before the goal attempt.",
        reference: "Issurin VB. New horizons for the methodology and physiology of training periodization. Sports Med. 2010;40(3):189–206.",
    },
    Basis {
        topic_pt: "Corrida + caminhada",
        topic_en: "Run + walk",
        use_pt: "Quando o ritmo leve fica muito lento (a partir de 9:00/km), a corrida leve vira blocos de trote com 1 minuto de caminhada, para manter a FC no limite. Num estudo com maratonistas amadores, a estratégia deu tempos parecidos com menos dor muscular e fadiga, mas não reduziu o estresse cardíaco da prova inteira; aqui ela é uma regra prática para respeitar o limite de FC.",
        use_en: "When the easy pace is very slow (9:00/km or slower), easy runs become jog blocks with 1-minute walks, to keep HR within the cap. In a study of amateur marathoners the strategy gave similar times with less muscle pain and fatigue, but did not lower the cardiac stress of the whole race; here it is a practical rule to respect the HR cap.",
        reference: "Galloway J. Galloway's Book on Running. 2nd ed. Shelter Publications; 2002. · Hottenrott K, Ludyga S, Schulze S, Gronwald T, Jäger FS. Does a run/walk strategy decrease cardiac stress during a marathon in non-elite runners? J Sci Med Sport. 2016;19(1).",
    },
    Basis {
        topic_pt: "Regras próprias do FitLedger (não vêm de um estudo)",
        topic_en: "FitLedger's own rules (not from a study)",
        use_pt: "Treinos dimensionados pela mediana das últimas 3 corridas; teste de evolução a cada ~18 dias; pausa de mais de 14 dias = retomada com treino menor; melhora estimada de 0,8% por semana no tempo e de 0,4 bpm por semana na FC (com 3 corridas por semana) quando ainda não há tendência, proporcional à raiz da frequência; 4 treinos leves de base para quem tem pouco histórico; deriva acima de 8% e mais de 30% do tempo acima da zona 2 como sinais de alerta. São escolhas conservadoras, refeitas com os seus dados a cada corrida.",
        use_en: "Sessions sized from the median of the last 3 runs; a progress check every ~18 days; a break of more than 14 days = a comeback with a smaller session; expected improvement of 0.8% a week in time and 0.4 bpm a week in HR (at 3 runs a week) when there is no trend yet, scaled by the square root of the frequency; 4 easy base sessions with little history; drift above 8% and more than 30% of the time above zone 2 as warning signs. These are conservative choices, redone with your data after every run.",
        reference: "",
    },
];
