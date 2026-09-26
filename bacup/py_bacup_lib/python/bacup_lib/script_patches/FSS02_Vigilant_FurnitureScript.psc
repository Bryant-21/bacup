Function ReconcileRoverSetup()
    Quest owner = GetOwningQuest()
    If owner == None || !owner.IsRunning() || owner.IsStageDone(100)
        CancelTimer(1)
        Return
    EndIf

    ObjectReference fixMarker = GetReference()
    Actor roverActor = None
    If Rover != None
        roverActor = Rover.GetActorReference()
    EndIf
    If fixMarker != None && roverActor != None && roverActor.GetDistance(fixMarker) <= 200.0
        owner.SetStage(100)
        Return
    EndIf
    StartTimer(2.0, 1)
EndFunction

Event OnAliasInit()
    ReconcileRoverSetup()
EndEvent

Event OnLoad()
    ReconcileRoverSetup()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1
        ReconcileRoverSetup()
    EndIf
EndEvent

Event OnAliasShutdown()
    CancelTimer(1)
EndEvent
