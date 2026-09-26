; Tutorial_Crafting_PlayerConnect reads the Started value to decide whether to
; restart this tutorial, so the material kit is granted only on the first start.
Function Fragment_Stage_0010_Item_00()
    Actor playerRef = Alias_PlayerAlias.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None && (Tutorial_ArmorCraftingStarted == None || playerRef.GetValue(Tutorial_ArmorCraftingStarted) < 1.0)
        If Tutorial_ArmorCraftingStarted != None
            playerRef.SetValue(Tutorial_ArmorCraftingStarted, 1.0)
        EndIf
        If Tutorial_ArmorCrafting_Materials != None
            playerRef.AddItem(Tutorial_ArmorCrafting_Materials, 1, False)
        EndIf
    EndIf
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveCompleted(10)
EndFunction
