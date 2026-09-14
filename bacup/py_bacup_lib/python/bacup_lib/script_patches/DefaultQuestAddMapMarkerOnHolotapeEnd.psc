Event OnQuestInit()
    RebuildHolotapeMapMarkerFilters()
    Actor player = Game.GetPlayer()
    If player
        RegisterForRemoteEvent(player, "OnItemAdded")
    EndIf
EndEvent

Event OnQuestShutdown()
    Actor player = Game.GetPlayer()
    If player
        UnregisterForRemoteEvent(player, "OnItemAdded")
    EndIf
    RemoveAllInventoryEventFilters()
EndEvent

Function RebuildHolotapeMapMarkerFilters()
    RemoveAllInventoryEventFilters()
    Int index = 0
    While HolotapeMapMarkerData != None && index < HolotapeMapMarkerData.Length
        Holotape tapeFilter = HolotapeMapMarkerData[index].Tape
        If tapeFilter != None && !HasEarlierHolotapeMapMarkerFilter(tapeFilter, index)
            AddInventoryEventFilter(tapeFilter)
        EndIf
        index += 1
    EndWhile
EndFunction

Bool Function HasEarlierHolotapeMapMarkerFilter(Holotape tapeFilter, Int beforeIndex)
    Int index = 0
    While HolotapeMapMarkerData != None && index < beforeIndex
        If HolotapeMapMarkerData[index].Tape == tapeFilter
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Event ObjectReference.OnHolotapePlay(ObjectReference akSender, ObjectReference akTerminalRef)
    ProcessHolotape(GetPlayedHolotape(akSender))
EndEvent

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akSender == Game.GetPlayer() && aiItemCount > 0
        ProcessHolotape(akBaseItem as Holotape)
    EndIf
EndEvent

Function ProcessHolotape(Holotape playedTape)
    Int index = 0
    While playedTape && HolotapeMapMarkerData && index < HolotapeMapMarkerData.Length
        If HolotapeMapMarkerData[index].Tape == playedTape && HolotapeMapMarkerData[index].MapMarker
            ; FO4 has no base-form play or completion event, so reveal on pickup.
            HolotapeMapMarkerData[index].MapMarker.AddToMap()
        EndIf
        index += 1
    EndWhile
EndFunction

Holotape Function GetPlayedHolotape(ObjectReference akSender)
    If akSender
        Return akSender.GetBaseObject() as Holotape
    EndIf
    Return None
EndFunction
