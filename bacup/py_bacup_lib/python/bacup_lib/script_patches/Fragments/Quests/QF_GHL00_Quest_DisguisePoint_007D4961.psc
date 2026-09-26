Function Fragment_Stage_0100_Item_00()
    Alias_Player.ForceRefIfEmpty(Game.GetPlayer())
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction
