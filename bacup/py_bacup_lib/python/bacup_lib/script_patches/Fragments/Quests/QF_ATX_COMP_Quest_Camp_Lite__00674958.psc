Function Fragment_Stage_0000_Item_00()
    Game.GetPlayer().SetValue(ATX_COMP_AV_NukaAgent_Variable01, 1.0)
EndFunction

Function Fragment_Stage_0010_Item_00()
    Game.GetPlayer().SetValue(AV_Intro, 1.0)
EndFunction

Function Fragment_Stage_0030_Item_00()
    Actor player = Game.GetPlayer()
    If NukaCola && player.GetItemCount(NukaCola) > 0
        player.RemoveItem(NukaCola, 1)
        player.ModValue(AffinitySystem, 1.0)
    EndIf
EndFunction
