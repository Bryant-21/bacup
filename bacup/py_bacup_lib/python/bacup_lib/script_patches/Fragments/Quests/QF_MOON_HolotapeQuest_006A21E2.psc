Function Fragment_Stage_0100_Item_00()
    If Alias_Player
        Alias_Player.ForceRefIfEmpty(Game.GetPlayer())
    EndIf
    If Moon_SQ01_AV_ToysCurrent
        Game.GetPlayer().SetValue(Moon_SQ01_AV_ToysCurrent, 0.0)
    EndIf
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200)
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0410_Item_00()
    SetToyCount(1)
EndFunction

Function Fragment_Stage_0420_Item_00()
    SetToyCount(2)
EndFunction

Function Fragment_Stage_0430_Item_00()
    SetToyCount(3)
EndFunction

Function Fragment_Stage_0440_Item_00()
    SetToyCount(4)
EndFunction

Function Fragment_Stage_0450_Item_00()
    SetToyCount(5)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetToyCount(5)
    SetObjectiveCompleted(300)
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(400)
EndFunction

Function Fragment_Stage_0610_Item_00()
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0620_Item_00()
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(500)
EndFunction

Function Fragment_Stage_9000_Item_00()
    RemoveToy(Moon_SQ01_Toy_1)
    RemoveToy(Moon_SQ01_Toy_2)
    RemoveToy(Moon_SQ01_Toy_3)
    RemoveToy(Moon_SQ01_Toy_4)
    RemoveToy(Moon_SQ01_Toy_5)
    Stop()
EndFunction

Function SetToyCount(Int count)
    If Moon_SQ01_AV_ToysCurrent
        Game.GetPlayer().SetValue(Moon_SQ01_AV_ToysCurrent, count)
    EndIf
EndFunction

Function RemoveToy(Form toy)
    If toy
        Game.GetPlayer().RemoveItem(toy, 1, True)
    EndIf
EndFunction
