Function ReconcileQuestDynamite()
    Quest host = GetOwningQuest()
    Actor player = GetActorReference()
    If host == None || !host.IsRunning() || host.IsCompleted() || host.IsStageDone(1400) || player == None
        Return
    EndIf
    If LocMountainsUncannyCavernsInteriorLocation == None || !player.IsInLocation(LocMountainsUncannyCavernsInteriorLocation) || Alias_QO_Weap_Dynamite == None
        Return
    EndIf
    ObjectReference dynamite = Alias_QO_Weap_Dynamite.GetReference()
    If dynamite == None || dynamite.GetContainer() != player || dynamite.GetBaseObject() != DynamiteBundleGrenade
        Return
    EndIf
    Fragments:Quests:QF_BS02_MQ01_Penance_005F38F1 fragments = host as Fragments:Quests:QF_BS02_MQ01_Penance_005F38F1
    If fragments == None || fragments.BS02_MQ01_Penance_DynamiteBundleGrenade == None
        Return
    EndIf
    Alias_QO_Weap_Dynamite.Clear()
    ObjectReference questBundle = player.PlaceAtMe(fragments.BS02_MQ01_Penance_DynamiteBundleGrenade, 1, True)
    If questBundle == None
        Alias_QO_Weap_Dynamite.ForceRefTo(dynamite)
        Return
    EndIf
    Alias_QO_Weap_Dynamite.ForceRefTo(questBundle)
    player.RemoveItem(dynamite, 1, True)
    player.AddItem(questBundle, 1, True)
EndFunction

Event OnAliasInit()
    AddInventoryEventFilter(DynamiteBundleGrenade)
    ReconcileQuestDynamite()
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akBaseItem == DynamiteBundleGrenade
        ReconcileQuestDynamite()
    EndIf
EndEvent

Event OnLocationChange(Location akOldLoc, Location akNewLoc)
    ReconcileQuestDynamite()
EndEvent

Event OnPlayerLoadGame()
    OnAliasInit()
EndEvent

Event OnAliasShutdown()
    RemoveAllInventoryEventFilters()
EndEvent
