Event OnQuestInit()
    Actor player = Alias_currentPlayer.GetActorRef()
    if player != None
        RegisterForRemoteEvent(player, "OnLocationChange")
    endif
EndEvent

Event OnStageSet(int auiStageID, int auiItemID)
    if auiStageID == StageToWatch
        CheckPlayerLocation()
    endif
EndEvent

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
    if akSender == Alias_currentPlayer.GetActorRef() && akNewLoc == Vault79Location.GetLocation()
        CheckPlayerLocation()
    endif
EndEvent

Function CheckPlayerLocation()
    Actor player = Alias_currentPlayer.GetActorRef()
    if player == None || GetStage() != StageToWatch
        return
    endif
    if player.GetCurrentLocation() != Vault79Location.GetLocation()
        return
    endif

    Utility.Wait(Delay as float)
    if GetStage() == StageToWatch && player.GetCurrentLocation() == Vault79Location.GetLocation()
        SetStage(StageToSet)
    endif
EndFunction
