Event OnInit()
    GoToState("normal")
EndEvent

Event OnLoad()
    If IsDead()
        ClearTantrum()
    ElseIf GetState() == "tantrum"
        StartTimer(TantrumTime, 1)
    Else
        GoToState("normal")
        StartTimer(0.5, 0)
    EndIf
EndEvent

Event OnUnload()
    ClearTantrum()
    GoToState("normal")
    CancelTimer(0)
EndEvent

Event OnReset()
    ClearTantrum()
    Tantrum1_Triggered = False
    Tantrum2_Triggered = False
    Tantrum3_Triggered = False
    GoToState("normal")
EndEvent

Event OnDying(Actor akKiller)
    ClearTantrum()
    GoToState("normal")
EndEvent

Event OnCombatStateChanged(Actor akTarget, Int aeCombatState)
    If aeCombatState == 1 && !IsDead() && GetState() == "normal"
        StartTimer(0.5, 0)
    EndIf
EndEvent

Function ClearTantrum()
    CancelTimer(0)
    CancelTimer(1)
    If BS02_MQ05_Catalyst_BehemothIgnoreCombatKeyword != None
        RemoveKeyword(BS02_MQ05_Catalyst_BehemothIgnoreCombatKeyword)
    EndIf
    If TantrumDisplaySpell != None
        DispelSpell(TantrumDisplaySpell)
    EndIf
    EvaluatePackage()
EndFunction

State normal
    Event OnBeginState(String asOldState)
        If !IsDead() && Is3DLoaded()
            StartTimer(0.5, 0)
        EndIf
    EndEvent

    Event OnTimer(Int aiTimerID)
        If aiTimerID != 0 || IsDead() || !Is3DLoaded() || !IsInCombat()
            Return
        EndIf
        ActorValue health = Game.GetFormFromFile(0x000002D4, "Fallout4.esm") as ActorValue
        If health == None
            Return
        EndIf
        Float remaining = GetValuePercentage(health)
        If !Tantrum1_Triggered && remaining <= Tantrum1_HealthPercentage
            Tantrum1_Triggered = True
            GoToState("tantrum")
        ElseIf !Tantrum2_Triggered && remaining <= Tantrum2_HealthPercentage
            Tantrum2_Triggered = True
            GoToState("tantrum")
        ElseIf !Tantrum3_Triggered && remaining <= Tantrum3_HealthPercentage
            Tantrum3_Triggered = True
            GoToState("tantrum")
        ElseIf !Tantrum3_Triggered
            StartTimer(0.5, 0)
        EndIf
    EndEvent
EndState

State tantrum
    Event OnBeginState(String asOldState)
        If IsDead()
            GoToState("normal")
            Return
        EndIf
        If BS02_MQ05_Catalyst_BehemothIgnoreCombatKeyword != None
            AddKeyword(BS02_MQ05_Catalyst_BehemothIgnoreCombatKeyword)
        EndIf
        EvaluatePackage()
        If TantrumDisplaySpell != None
            TantrumDisplaySpell.Cast(Self, Self)
        EndIf
        StartTimer(TantrumTime, 1)
    EndEvent

    Event OnTimer(Int aiTimerID)
        If aiTimerID == 1
            GoToState("normal")
        EndIf
    EndEvent

    Event OnEndState(String asNewState)
        ClearTantrum()
    EndEvent
EndState
