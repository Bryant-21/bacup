; The converted Player alias has no fill; AliasSetStageOnItemEquipped on alias 0 sets 9000 once it holds the player.
Function Fragment_Stage_0100_Item_00()
    Actor player = Game.GetPlayer()
    AliasSetStageOnItemEquipped playerAlias = GetAlias(0) as AliasSetStageOnItemEquipped
    If playerAlias == None
        SetObjectiveDisplayed(10, True, True)
        Return
    EndIf
    playerAlias.ForceRefIfEmpty(player)
    If playerAlias.ItemToCheck != None && player.IsEquipped(playerAlias.ItemToCheck)
        SetStage(9000)
        Return
    EndIf
    SetObjectiveDisplayed(10, True, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    If IsObjectiveDisplayed(10)
        SetObjectiveCompleted(10, True)
    EndIf
    Stop()
EndFunction
