Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == Deathtrap03BeginStage
        StartTimer(WeaselDeathtrapSayTimerLength, WeaselDeathtrapSayTimerID)
    ElseIf auiStageID == StopDetonationStage
        StartTimer(DetectionTimerLength, DetectionTimerID)
    ElseIf auiStageID == LouNoticedStage || auiStageID == AllBreakersOffStage || auiStageID == PlayerTalkedToLouStage
        CancelTimer(DetectionTimerID)
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(DetectionTimerID)
    CancelTimer(WeaselDeathtrapSayTimerID)
    CancelTimer(SignalTimerID)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == WeaselDeathtrapSayTimerID
        Actor weaselActor = None
        If Weasel != None
            weaselActor = Weasel.GetActorReference()
        EndIf
        If weaselActor != None && W05_MQR_201P_Weasel_Deathtrap03_Comment02 != None
            weaselActor.Say(W05_MQR_201P_Weasel_Deathtrap03_Comment02, weaselActor, False, Game.GetPlayer())
        EndIf
        If !IsStageDone(Deathtrap03WeaselStage)
            SetStage(Deathtrap03WeaselStage)
        EndIf
    ElseIf aiTimerID == DetectionTimerID
        Actor louActor = None
        Actor playerActor = None
        If Lou != None
            louActor = Lou.GetActorReference()
        EndIf
        If currentPlayer != None
            playerActor = currentPlayer.GetActorReference()
        EndIf
        If playerActor == None
            playerActor = Game.GetPlayer()
        EndIf
        If IsStageDone(LouNoticedStage) || IsStageDone(AllBreakersOffStage) || IsStageDone(PlayerTalkedToLouStage)
            Return
        EndIf
        If louActor == None || playerActor == None || louActor.IsDead()
            Return
        EndIf
        If playerActor.GetDistance(louActor) <= LouDistance && playerActor.IsDetectedBy(louActor)
            If W05_MQR_201P_LouSaysTopic_SneakFail != None
                louActor.Say(W05_MQR_201P_LouSaysTopic_SneakFail, louActor, False, playerActor)
            EndIf
            SetStage(LouNoticedStage)
            Return
        EndIf
        StartTimer(DetectionTimerLength, DetectionTimerID)
    EndIf
EndEvent
