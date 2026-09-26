Event OnQuestInit()
    PlayerRef = MyPlayer.GetActorReference()
    canStir = true
    If Activator_Ladle != None
        RegisterForRemoteEvent(Activator_Ladle, "OnActivate")
    EndIf
EndEvent

Event ReferenceAlias.OnActivate(ReferenceAlias akSender, ObjectReference akActionRef)
    If akSender != Activator_Ladle || akActionRef != PlayerRef || !IsStageDone(368) || IsStageDone(380)
        Return
    EndIf

    If canStir
        canStir = false
        If XPD_Fuel_Recipe_StirMessage != None
            XPD_Fuel_Recipe_StirMessage.Show()
        EndIf
        CancelTimer(1)
        StartTimer(incrementTimer, 1)
        StartTimer(stirCooldown, 2)
    ElseIf XPD_Fuel_Recipe_CantStir != None
        XPD_Fuel_Recipe_CantStir.Show()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    ; CancelTimer does not recall a tick that has already fired, so one more can
    ; still arrive after OnQuestShutdown has stopped the loop. Bail out before
    ; touching any stage rather than acting on behalf of a stopped quest.
    If !IsRunning()
        Return
    EndIf

    If aiTimerID == 1 && IsStageDone(308) && !IsStageDone(370) && !IsStageDone(375)
        SetStage(375)
    ElseIf aiTimerID == 2
        canStir = true
    ElseIf aiTimerID == 3
        AdvanceKitchen()
        If IsRunning() && !IsStageDone(9000)
            StartTimer(3.0, 3)
        EndIf
    EndIf
EndEvent

Bool Function PlayerIsBeside(Int aiAliasID)
    If PlayerRef == None
        PlayerRef = MyPlayer.GetActorReference()
    EndIf
    ReferenceAlias target = GetAlias(aiAliasID) as ReferenceAlias
    If PlayerRef == None || target == None
        Return False
    EndIf
    ObjectReference ref = target.GetReference()
    Return ref != None && PlayerRef.GetDistance(ref) <= 384.0
EndFunction

; Every step Esme and the two settlement representatives drove through dialogue
; and bound scenes is dead in the conversion. Standing with them advances one
; step per tick, which walks the quest into the parts the records still drive:
; the ingredient hunt, the cooking timer and the stew hand-off.
Function AdvanceKitchen()
    If IsStageDone(450)
        If !IsStageDone(530) && PlayerIsBeside(32)
            SetStage(530)
        ElseIf !IsStageDone(580) && PlayerIsBeside(31)
            SetStage(580)
        EndIf
        Return
    EndIf

    If !PlayerIsBeside(3)
        Return
    EndIf

    If IsStageDone(375)
        If !IsStageDone(380)
            SetStage(380)
        EndIf
    ElseIf IsStageDone(370)
        SetStage(450)
    ElseIf !IsStageDone(300)
        SetStage(300)
    ElseIf !IsStageDone(305)
        SetStage(305)
    ElseIf !IsStageDone(308)
        SetStage(308)
        SetStage(309)
    EndIf
EndFunction

Function CheckIngredientCollection()
    If IsStageDone(310) && IsStageDone(320) && IsStageDone(330) && !IsStageDone(340)
        SetStage(340)
    EndIf
EndFunction

Function CheckIngredientPreparation()
    If IsStageDone(345) && IsStageDone(350) && IsStageDone(355) && !IsStageDone(360)
        SetStage(360)
    EndIf
EndFunction

Function CheckSpices()
    If IsStageDone(362) && IsStageDone(364) && !IsStageDone(366)
        SetStage(366)
    EndIf
EndFunction

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 100
        SetObjectiveDisplayed(20)
        CancelTimer(3)
        StartTimer(3.0, 3)
    ElseIf auiStageID == 300
        SetObjectiveCompleted(20)
        SetObjectiveDisplayed(25)
    ElseIf auiStageID == 305
        SetObjectiveCompleted(25)
        SetObjectiveDisplayed(28)
    ElseIf auiStageID == 308
        CancelTimer(1)
        StartTimer(incrementTimer, 1)
    ElseIf auiStageID == 309
        SetObjectiveCompleted(28)
        SetObjectiveDisplayed(TimerObjective)
        SetObjectiveDisplayed(31)
        SetObjectiveDisplayed(32)
        SetObjectiveDisplayed(33)
        CancelTimer(1)
        StartTimer(incrementTimer, 1)
    ElseIf auiStageID == 310
        SetObjectiveCompleted(33)
        CheckIngredientCollection()
    ElseIf auiStageID == 320
        SetObjectiveCompleted(32)
        CheckIngredientCollection()
    ElseIf auiStageID == 330
        SetObjectiveCompleted(31)
        CheckIngredientCollection()
    ElseIf auiStageID == 340
        SetObjectiveDisplayed(34)
        SetObjectiveDisplayed(35)
        SetObjectiveDisplayed(36)
    ElseIf auiStageID == 345
        SetObjectiveCompleted(34)
        CheckIngredientPreparation()
    ElseIf auiStageID == 350
        SetObjectiveCompleted(35)
        CheckIngredientPreparation()
    ElseIf auiStageID == 355
        SetObjectiveCompleted(36)
        CheckIngredientPreparation()
    ElseIf auiStageID == 360
        SetObjectiveDisplayed(37)
    ElseIf auiStageID == 361
        SetObjectiveCompleted(37)
        SetObjectiveDisplayed(40)
        SetObjectiveDisplayed(41)
    ElseIf auiStageID == 362
        SetObjectiveCompleted(41)
        CheckSpices()
    ElseIf auiStageID == 364
        SetObjectiveCompleted(40)
        CheckSpices()
    ElseIf auiStageID == 366
        SetObjectiveDisplayed(42)
    ElseIf auiStageID == 368
        SetObjectiveCompleted(42)
        SetObjectiveDisplayed(StirObjective)
        If !IsStageDone(370)
            SetStage(370)
        EndIf
    ElseIf auiStageID == 370 || auiStageID == 375
        CancelTimer(1)
        CancelTimer(2)
        SetObjectiveCompleted(TimerObjective)
        SetObjectiveCompleted(StirObjective)
        SetObjectiveDisplayed(50)
    ElseIf auiStageID == 380
        SetObjectiveCompleted(50)
        If !IsStageDone(9000)
            SetStage(9000)
        EndIf
    ElseIf auiStageID == 450
        SetObjectiveCompleted(50)
        SetObjectiveDisplayed(60)
        SetObjectiveDisplayed(70)
    ElseIf auiStageID == 530
        SetObjectiveCompleted(60)
        SetObjectiveDisplayed(70, false)
        If !IsStageDone(9000)
            SetStage(9000)
        EndIf
    ElseIf auiStageID == 580
        SetObjectiveCompleted(70)
        SetObjectiveDisplayed(60, false)
        If !IsStageDone(9000)
            SetStage(9000)
        EndIf
    ElseIf auiStageID == 9000
        CancelTimer(3)
        CompleteAllObjectives()
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(1)
    CancelTimer(2)
    CancelTimer(3)
    If Activator_Ladle != None
        UnregisterForRemoteEvent(Activator_Ladle, "OnActivate")
    EndIf
EndEvent
