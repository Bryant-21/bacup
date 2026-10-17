; Tadpole Archery Test. FO76's server held the start (objective 15, M01C_Archery_WaitTimer_Short) for other players,
; then set 110; the test needs every ArcheryTargets bullseye trigger hit with a crossbow or bow before
; M01C_Archery_QuestTimer (B21:QuestTimer, 110 -> 1000) runs out. A bolt or arrow entering a bullseye sphere, or a
; registered hit on it, scores that target once (objectives 31-35, BullseyeCount).
Event OnQuestInit()
    QuestTimerValue = 0.0
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 100
        SetVariable("BullseyesRequired", ArcheryTargets.Length)
        SetVariable("BullseyeCount", 0)
        SetObjectiveDisplayed(15, True)
        Float waitSeconds = 10.0
        GlobalVariable shortWait = Game.GetFormFromFile(0x42CF03, "SeventySix.esm") as GlobalVariable
        If shortWait != None && shortWait.GetValue() > 0.0
            waitSeconds = shortWait.GetValue()
        EndIf
        StartTimer(waitSeconds, 1)
    ElseIf auiStageID == 110
        BeginTest()
    ElseIf auiStageID == 1000 || auiStageID == 1100 || auiStageID == 1200 || auiStageID == 9000
        EndTest()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1 && IsRunning() && !IsStageDone(110)
        SetStage(110)
    EndIf
EndEvent

Function BeginTest()
    SetObjectiveCompleted(10, True)
    SetObjectiveCompleted(15, True)
    SetObjectiveDisplayed(20, True)
    SetObjectiveDisplayed(30, True)
    Actor player = Alias_Player.GetActorReference()
    Int index = 0
    While index < ArcheryTargets.Length
        SetObjectiveDisplayed(31 + index, True)
        ObjectReference target = ArcheryTargets[index].GetReference()
        If target != None
            RegisterForRemoteEvent(target, "OnTriggerEnter")
            If player != None
                RegisterForHitEvent(target, player)
            EndIf
        EndIf
        index += 1
    EndWhile
    If M01C_Archery_TimeWarnings != None
        M01C_Archery_TimeWarnings.Start()
    EndIf
EndFunction

Function EndTest()
    UnregisterForAllHitEvents()
    Int index = 0
    While index < ArcheryTargets.Length
        ObjectReference target = ArcheryTargets[index].GetReference()
        If target != None
            UnregisterForRemoteEvent(target, "OnTriggerEnter")
        EndIf
        index += 1
    EndWhile
    If M01C_Archery_TimeWarnings != None && M01C_Archery_TimeWarnings.IsPlaying()
        M01C_Archery_TimeWarnings.Stop()
    EndIf
    SetObjectiveDisplayed(40, False)
    Actor player = Alias_Player.GetActorReference()
    If player != None && M01C_Archery_IsInTrigger != None
        player.SetValue(M01C_Archery_IsInTrigger, 0.0)
    EndIf
EndFunction

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef != None && (akActionRef as Actor) == None && PlayerHasArcheryWeapon()
        ScoreTarget(akSender)
    EndIf
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
    If IsArcheryWeapon(akSource as Weapon)
        ScoreTarget(akTarget)
    ElseIf IsStageDone(110) && !IsStageDone(9000)
        Actor player = Alias_Player.GetActorReference()
        If player != None
            RegisterForHitEvent(akTarget, player)
        EndIf
    EndIf
EndEvent

Function ScoreTarget(ObjectReference akTarget)
    If !IsStageDone(110) || IsStageDone(1000) || IsStageDone(1100) || IsStageDone(9000)
        Return
    EndIf
    Int index = 0
    While index < ArcheryTargets.Length
        If ArcheryTargets[index].GetReference() == akTarget
            If !IsObjectiveCompleted(31 + index)
                SetObjectiveCompleted(31 + index, True)
                If OBJDrossTossSuccessfulThrow != None
                    OBJDrossTossSuccessfulThrow.Play(akTarget)
                EndIf
                UpdateScore()
            EndIf
            Return
        EndIf
        index += 1
    EndWhile
EndFunction

Function UpdateScore()
    Int scored = 0
    Int index = 0
    While index < ArcheryTargets.Length
        If IsObjectiveCompleted(31 + index)
            scored += 1
        EndIf
        index += 1
    EndWhile
    SetVariable("BullseyeCount", scored)
    If scored >= ArcheryTargets.Length
        SetObjectiveCompleted(20, True)
        SetObjectiveCompleted(30, True)
        SetStage(9000)
    ElseIf scored * 2 >= ArcheryTargets.Length && M01C_Archery_MidEvent_Chirps != None && !M01C_Archery_MidEvent_Chirps.IsPlaying()
        M01C_Archery_MidEvent_Chirps.Start()
    EndIf
EndFunction

Bool Function PlayerHasArcheryWeapon()
    Actor player = Alias_Player.GetActorReference()
    Return player != None && IsArcheryWeapon(player.GetEquippedWeapon())
EndFunction

Bool Function IsArcheryWeapon(Weapon akWeapon)
    If akWeapon == None
        Return False
    EndIf
    If akWeapon == Crossbow
        Return True
    EndIf
    Keyword bowType = Game.GetFormFromFile(0x17F4FB, "SeventySix.esm") as Keyword
    Keyword crossbowType = Game.GetFormFromFile(0x34EBD3, "SeventySix.esm") as Keyword
    Return (bowType != None && akWeapon.HasKeyword(bowType)) || (crossbowType != None && akWeapon.HasKeyword(crossbowType))
EndFunction

Function SetVariable(String asName, Int aiValue)
    B21:QuestVariables questVariables = (Self as Quest) as B21:QuestVariables
    If questVariables != None
        questVariables.SetVariable(asName, aiValue as Float)
    EndIf
EndFunction
