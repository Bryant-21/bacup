; Timer IDs: 1 generator repair window, 2 pre-combat check fallback, 3 event shutdown.
Event OnQuestInit()
    bGeneratorState = False
    Quest owner = Self as Quest
    B21:QuestTimer questTimer = owner as B21:QuestTimer
    If questTimer != None
        RegisterForCustomEvent(questTimer, "QuestTimerEnded")
    EndIf
EndEvent

; The defense clock ends the event: an intact generator wins, a destroyed one loses.
Event B21:QuestTimer.QuestTimerEnded(B21:QuestTimer akSender, Var[] akArgs)
    If !IsRunning() || !IsStageDone(325) || EventEnded()
        Return
    EndIf
    If GeneratorNeedsRepair()
        SetStage(8900)
    Else
        SetStage(700)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1
        If IsRunning() && IsStageDone(325) && !EventEnded() && GeneratorNeedsRepair()
            SetStage(9900)
        EndIf
    ElseIf aiTimerID == 2
        If IsRunning() && IsStageDone(300) && !IsStageDone(325) && !EventEnded()
            SetStage(325)
        EndIf
    ElseIf aiTimerID == 3
        If IsRunning()
            Stop()
        EndIf
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelDefenseTimers()
    CancelTimer(3)
EndEvent

Bool Function EventEnded()
    Return IsStageDone(700) || IsStageDone(8900) || IsStageDone(9000) || IsStageDone(9900)
EndFunction

ObjectReference Function GeneratorRef()
    ReferenceAlias generatorAlias = GetAlias(1) as ReferenceAlias
    If generatorAlias == None
        Return None
    EndIf
    Return generatorAlias.GetReference()
EndFunction

Bool Function GeneratorNeedsRepair()
    Return DefaultAliasOnObjectRepaired.NeedsRepair(GeneratorRef())
EndFunction

; The event opens with the generator broken so the players have something to repair.
Function PrepareGenerator()
    If IsStageDone(200) || EventEnded()
        Return
    EndIf
    ObjectReference generator = GeneratorRef()
    If generator == None || !generator.Is3DLoaded() || DefaultAliasOnObjectRepaired.NeedsRepair(generator)
        Return
    EndIf
    ; The converted destructible carries FO76's 9,999,999 placeholder health (its health global was dropped).
    generator.DamageObject(10000000.0)
EndFunction

Function StartPreCombatFallback()
    StartTimer(fPreCombatTimer, 2)
EndFunction

Function BeginDefense()
    CancelTimer(2)
    bGeneratorState = True
    UpdateGeneratorState()
EndFunction

Function UpdateGeneratorState()
    If !IsRunning() || EventEnded()
        Return
    EndIf
    Bool operational = !GeneratorNeedsRepair()
    If !IsStageDone(325)
        bGeneratorState = operational
        Return
    EndIf
    If operational == bGeneratorState
        Return
    EndIf
    bGeneratorState = operational
    If operational
        CancelTimer(1)
        SetObjectiveDisplayed(350, False)
    Else
        SetObjectiveCompleted(350, False)
        SetObjectiveDisplayed(350, True, True)
        StartTimer(fRepairTimer, 1)
    EndIf
EndFunction

Function CancelDefenseTimers()
    CancelTimer(1)
    CancelTimer(2)
EndFunction

Function ScheduleShutdown()
    CancelDefenseTimers()
    StartTimer(30.0, 3)
EndFunction
