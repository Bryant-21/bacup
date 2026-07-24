Event OnQuestInit()
    Actor ownerActor = InstanceOwner.GetActorReference()
    If ownerActor == None || TargetLocation == None
        Return
    EndIf
    If !ownerActor.IsInLocation(TargetLocation)
        Stop()
        Return
    EndIf
    RegisterForRemoteEvent(ownerActor, "OnLocationChange")
EndEvent

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
    If akSender != None && TargetLocation != None && !akSender.IsInLocation(TargetLocation)
        UnregisterForRemoteEvent(akSender, "OnLocationChange")
        Stop()
    EndIf
EndEvent

Event OnQuestShutdown()
    UnregisterForAllRemoteEvents()
EndEvent
