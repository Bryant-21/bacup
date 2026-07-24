Function Fragment_Stage_0100_Item_00()
    If Alias_Player
        Alias_Player.ForceRefIfEmpty(Game.GetPlayer())
    EndIf
    SetObjectiveDisplayed(5)
EndFunction

Function Fragment_Stage_0110_Item_00()
    Game.GetPlayer().SetValue(E06_PlayerHasHolotape, 1.0)
    SetObjectiveCompleted(5)
    SetObjectiveDisplayed(8)
EndFunction

Function Fragment_Stage_0125_Item_00()
    SetObjectiveCompleted(8)
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0150_Item_00()
    Game.GetPlayer().SetValue(E06_PlayerTalkedToMaggie, 1.0)
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(20)
    If Object_Holotape
        Game.GetPlayer().RemoveItem(Object_Holotape, 1, True)
    EndIf
    If E06_PlayerHasHolotape
        Game.GetPlayer().SetValue(E06_PlayerHasHolotape, 0.0)
    EndIf
    Stop()
EndFunction
