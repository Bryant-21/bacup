Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0300_Item_00()
    AddQuestItemIfMissing(XPD_Fuel_Recipe_ChefOutfit_NONPLAYABLE)
    StartBoundScene(XPD_Fuel_Recipe_Esme_ChefsCoat)
EndFunction

Function Fragment_Stage_0305_Item_00()
    StartBoundScene(XPD_Fuel_Recipe_Esme_FindIngredients01)
EndFunction

Function Fragment_Stage_0308_Item_00()
    StopBoundScene(XPD_Fuel_Recipe_Esme_ChefsCoat)
    StopBoundScene(XPD_Fuel_Recipe_Esme_FindIngredients01)
EndFunction

Function Fragment_Stage_0309_Item_00()
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0310_Item_00()
    SetObjectiveCompleted(33)
EndFunction

Function Fragment_Stage_0320_Item_00()
    SetObjectiveCompleted(32)
EndFunction

Function Fragment_Stage_0330_Item_00()
    SetObjectiveCompleted(31)
EndFunction

Function Fragment_Stage_0340_Item_00()
    SetObjectiveDisplayed(34)
    SetObjectiveDisplayed(35)
    SetObjectiveDisplayed(36)
    StartBoundScene(XPD_Fuel_Recipe_Esme_FindIngredients02)
EndFunction

Function Fragment_Stage_0343_Item_00()
    MoveAliasToMarker(Alias_MO_CarrotBasket, Alias_Marker_PrepareCarrots)
EndFunction

Function Fragment_Stage_0345_Item_00()
    SetObjectiveCompleted(34)
EndFunction

Function Fragment_Stage_0347_Item_00()
    MoveAliasToMarker(Alias_MO_TatoBasket, Alias_Marker_PrepareTatos)
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveCompleted(35)
EndFunction

Function Fragment_Stage_0353_Item_00()
    MoveAliasToMarker(Alias_MO_VenisonBasket, Alias_Marker_PrepareVenison)
EndFunction

Function Fragment_Stage_0355_Item_00()
    SetObjectiveCompleted(36)
EndFunction

Function Fragment_Stage_0360_Item_00()
    SetObjectiveDisplayed(37)
EndFunction

Function Fragment_Stage_0361_Item_00()
    SetObjectiveCompleted(37)
    SetObjectiveDisplayed(40)
    SetObjectiveDisplayed(41)
    StartBoundScene(XPD_Hub_Recipe_Esme_AddedIngredient)
EndFunction

Function Fragment_Stage_0362_Item_00()
    SetObjectiveCompleted(41)
EndFunction

Function Fragment_Stage_0364_Item_00()
    SetObjectiveCompleted(40)
EndFunction

Function Fragment_Stage_0366_Item_00()
    SetObjectiveDisplayed(42)
EndFunction

Function Fragment_Stage_0368_Item_00()
    SetObjectiveCompleted(42)
    SetObjectiveDisplayed(45)
EndFunction

Function Fragment_Stage_0370_Item_00()
    AddQuestItemIfMissing(XPD_Hub_Recipe_Soup)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0375_Item_00()
    AddQuestItemIfMissing(XPD_Hub_Recipe_SoupBurned)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0380_Item_00()
    RemoveQuestItem(XPD_Hub_Recipe_SoupBurned)
    RemoveQuestItem(XPD_Fuel_Recipe_ChefOutfit_NONPLAYABLE)
    SetObjectiveCompleted(50)
EndFunction

Function Fragment_Stage_0400_Item_00()
    AdvanceToStage(450)
EndFunction

Function Fragment_Stage_0410_Item_00()
    RemoveOneQuestItem(Stimpak)
    AdvanceToStage(450)
EndFunction

Function Fragment_Stage_0420_Item_00()
    RemoveOneQuestItem(Psycho)
    AdvanceToStage(450)
EndFunction

Function Fragment_Stage_0430_Item_00()
    RemoveOneQuestItem(Spices)
    AdvanceToStage(450)
EndFunction

Function Fragment_Stage_0450_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60)
    SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_0500_Item_00()
    AdvanceToStage(530)
EndFunction

Function Fragment_Stage_0510_Item_00()
    AdvanceToStage(530)
EndFunction

Function Fragment_Stage_0520_Item_00()
    AdvanceToStage(530)
EndFunction

Function Fragment_Stage_0530_Item_00()
    FinishSoupDelivery(60, 70)
EndFunction

Function Fragment_Stage_0550_Item_00()
    AdvanceToStage(580)
EndFunction

Function Fragment_Stage_0560_Item_00()
    AdvanceToStage(580)
EndFunction

Function Fragment_Stage_0570_Item_00()
    AdvanceToStage(580)
EndFunction

Function Fragment_Stage_0580_Item_00()
    FinishSoupDelivery(70, 60)
EndFunction

Function Fragment_Stage_9000_Item_00()
    RemoveQuestItem(XPD_Hub_Recipe_Soup)
    RemoveQuestItem(XPD_Hub_Recipe_SoupBurned)
    RemoveQuestItem(XPD_Fuel_Recipe_ChefOutfit_NONPLAYABLE)
    StopBoundScene(XPD_Fuel_Recipe_Esme_ChefsCoat)
    StopBoundScene(XPD_Fuel_Recipe_Esme_FindIngredients01)
    StopBoundScene(XPD_Fuel_Recipe_Esme_FindIngredients02)
    StopBoundScene(XPD_Hub_Recipe_Esme_AddedIngredient)
    CompleteAllObjectives()
EndFunction

Function AddQuestItemIfMissing(Form itemToAdd)
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && itemToAdd != None && playerRef.GetItemCount(itemToAdd) == 0
        playerRef.AddItem(itemToAdd, 1, true)
    EndIf
EndFunction

Function RemoveQuestItem(Form itemToRemove)
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && itemToRemove != None
        Int itemCount = playerRef.GetItemCount(itemToRemove)
        If itemCount > 0
            playerRef.RemoveItem(itemToRemove, itemCount, true)
        EndIf
    EndIf
EndFunction

Function RemoveOneQuestItem(Form itemToRemove)
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && itemToRemove != None && playerRef.GetItemCount(itemToRemove) > 0
        playerRef.RemoveItem(itemToRemove, 1, true)
    EndIf
EndFunction

Function MoveAliasToMarker(ReferenceAlias itemAlias, ReferenceAlias markerAlias)
    If itemAlias == None || markerAlias == None
        Return
    EndIf
    ObjectReference itemRef = itemAlias.GetReference()
    ObjectReference markerRef = markerAlias.GetReference()
    If itemRef != None && markerRef != None
        itemRef.MoveTo(markerRef)
    EndIf
EndFunction

Function StartBoundScene(Scene sceneToStart)
    If sceneToStart != None && !sceneToStart.IsPlaying()
        sceneToStart.Start()
    EndIf
EndFunction

Function StopBoundScene(Scene sceneToStop)
    If sceneToStop != None && sceneToStop.IsPlaying()
        sceneToStop.Stop()
    EndIf
EndFunction

Function FinishSoupDelivery(Int completedObjective, Int hiddenObjective)
    SetObjectiveCompleted(completedObjective)
    SetObjectiveDisplayed(hiddenObjective, false)
    RemoveQuestItem(XPD_Hub_Recipe_Soup)
    RemoveQuestItem(XPD_Fuel_Recipe_ChefOutfit_NONPLAYABLE)
EndFunction

Function AdvanceToStage(Int stageToSet)
    If !IsStageDone(stageToSet)
        SetStage(stageToSet)
    EndIf
EndFunction
