Function Fragment_Stage_0050_Item_00()
    If Scene_Startup_Idles != None && !Scene_Startup_Idles.IsPlaying()
        Scene_Startup_Idles.Start()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If Scene_Instructions != None && !Scene_Instructions.IsPlaying()
        Scene_Instructions.Start()
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    If Scene_Intro != None && !Scene_Intro.IsPlaying()
        Scene_Intro.Start()
    EndIf
EndFunction

Function Fragment_Stage_0120_Item_00()
    If Scene_Uncle != None && !Scene_Uncle.IsPlaying()
        Scene_Uncle.Start()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    If Scene_Bells_Louder != None && !Scene_Bells_Louder.IsPlaying()
        Scene_Bells_Louder.Start()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    If Scene_CollectMaterials != None && !Scene_CollectMaterials.IsPlaying()
        Scene_CollectMaterials.Start()
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    If Scene_CollectMaterials_More != None && !Scene_CollectMaterials_More.IsPlaying()
        Scene_CollectMaterials_More.Start()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    If Scene_TheRing != None && !Scene_TheRing.IsPlaying()
        Scene_TheRing.Start()
    EndIf
EndFunction
