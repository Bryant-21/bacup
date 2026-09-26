Function ReconcileEntry()
    myQI = GetOwningQuest()
    If FortAtlasDungeonLoc != None
        myDungeonLoc = FortAtlasDungeonLoc.GetLocation()
    EndIf
    Actor player = GetActorReference()
    Fragments:Quests:QF_BS01_FortAtlasDungeon01_M_005EF38C controller = myQI as Fragments:Quests:QF_BS01_FortAtlasDungeon01_M_005EF38C
    If player != None && controller != None && myDungeonLoc != None && player.IsInLocation(myDungeonLoc)
        controller.ReconcileDungeon()
    EndIf
EndFunction

Event OnAliasInit()
    Actor player = GetActorReference()
    If player != None
        RegisterForRemoteEvent(player, "OnLocationChange")
    EndIf
    ReconcileEntry()
EndEvent

Event OnPlayerLoadGame()
    OnAliasInit()
EndEvent

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
    If akSender == GetActorReference()
        ReconcileEntry()
    EndIf
EndEvent

Event OnAliasShutdown()
    UnregisterForAllEvents()
EndEvent
