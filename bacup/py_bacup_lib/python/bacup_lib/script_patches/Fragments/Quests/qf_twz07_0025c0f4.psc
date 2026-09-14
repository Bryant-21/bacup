Function Fragment_Stage_1000_Item_00()
    If TWZ07MayorMonsterDead && !TWZ07MayorMonsterDead.IsPlaying()
        TWZ07MayorMonsterDead.Start()
    EndIf
EndFunction
