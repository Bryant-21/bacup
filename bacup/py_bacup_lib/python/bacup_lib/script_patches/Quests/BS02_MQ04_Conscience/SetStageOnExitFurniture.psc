Event OnInit()
    myQI = GetOwningQuest()
    myPlayerRef = Alias_Player.GetActorReference()
EndEvent

Event OnExitFurniture(ObjectReference akActionRef)
    Actor localPlayer = Game.GetPlayer()
    If !myQI
        myQI = GetOwningQuest()
    EndIf
    If !myPlayerRef
        myPlayerRef = Alias_Player.GetActorReference()
    EndIf
    If akActionRef == localPlayer && myPlayerRef == localPlayer && myQI
        If myQI.IsStageDone(PrereqStage) && !myQI.IsStageDone(StageToSet)
            If !myQI.IsStageDone(ShutoffStage)
                myQI.SetStage(StageToSet)
            EndIf
        EndIf
    EndIf
EndEvent
