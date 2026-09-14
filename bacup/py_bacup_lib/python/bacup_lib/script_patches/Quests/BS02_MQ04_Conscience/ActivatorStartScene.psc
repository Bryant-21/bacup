Event OnInit()
    myQI = GetOwningQuest()
    myPlayerRef = Alias_Player.GetActorReference()
EndEvent

Event OnActivate(ObjectReference akActionRef)
    Actor localPlayer = Game.GetPlayer()
    If !myQI
        myQI = GetOwningQuest()
    EndIf
    If !myPlayerRef
        myPlayerRef = Alias_Player.GetActorReference()
    EndIf
    If akActionRef == localPlayer && myPlayerRef == localPlayer && myQI
        If myQI.IsStageDone(PrereqStage) && !myQI.IsStageDone(ShutoffStage)
            If SceneToPlay && !SceneToPlay.IsPlaying()
                SceneToPlay.Start()
            EndIf
        EndIf
    EndIf
EndEvent
