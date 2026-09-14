Event OnQuestInit()
    RegisterForMayorDistance()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        RegisterForMayorDistance()
    EndIf
EndEvent

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    Actor player = Game.GetPlayer()
    If player != None && CB04_Mayor != None && CB04_StartOnDistanceLessThanMarkerRef != None && ((akObj1 == player && akObj2 == CB04_StartOnDistanceLessThanMarkerRef) || (akObj2 == player && akObj1 == CB04_StartOnDistanceLessThanMarkerRef)) && !CB04_Mayor.IsRunning() && !CB04_Mayor.IsCompleted()
        If CB04_Mayor.Start() || CB04_Mayor.IsRunning()
            If !CB04_Mayor.IsStageDone(10)
                CB04_Mayor.SetStage(10)
            EndIf
        EndIf
    EndIf
EndEvent

Function RegisterForMayorDistance()
    Actor player = Game.GetPlayer()
    If player != None
        RegisterForRemoteEvent(player, "OnPlayerLoadGame")
        If CB04_Mayor != None && CB04_StartOnDistanceLessThanMarkerRef != None && !CB04_Mayor.IsRunning() && !CB04_Mayor.IsCompleted()
            RegisterForDistanceLessThanEvent(player, CB04_StartOnDistanceLessThanMarkerRef, CB04_RadioDistance)
        EndIf
    EndIf
EndFunction
