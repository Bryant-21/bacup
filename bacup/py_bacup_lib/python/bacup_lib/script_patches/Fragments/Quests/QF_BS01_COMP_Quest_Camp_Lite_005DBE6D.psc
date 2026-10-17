Function Fragment_Stage_0100_Item_00()
    Game.GetPlayer().SetValue(AV_Intro, 1.0)
EndFunction

Function Fragment_Stage_0200_Item_00()
    Actor playerRef = Game.GetPlayer()
    If ScorchbeastStew && playerRef.GetItemCount(ScorchbeastStew) > 0
        playerRef.RemoveItem(ScorchbeastStew, 1)
    EndIf
    playerRef.SetValue(AV_ScorchbeastStew, 1.0)
EndFunction
