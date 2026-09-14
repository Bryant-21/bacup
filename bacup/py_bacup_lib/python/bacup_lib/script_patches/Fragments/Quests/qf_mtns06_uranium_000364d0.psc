Function Fragment_Stage_0150_Item_00()
    If MTNS06_Uranium_PA_ActivityStart && !MTNS06_Uranium_PA_ActivityStart.IsPlaying()
        MTNS06_Uranium_PA_ActivityStart.Start()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    If MTNS06_Uranium_PA_EnemiesSpawning && !MTNS06_Uranium_PA_EnemiesSpawning.IsPlaying()
        MTNS06_Uranium_PA_EnemiesSpawning.Start()
    EndIf
EndFunction

Function Fragment_Stage_0201_Item_00()
    If MTNS06_Uranium_PA_BossSpawn && !MTNS06_Uranium_PA_BossSpawn.IsPlaying()
        MTNS06_Uranium_PA_BossSpawn.Start()
    EndIf
EndFunction

Function Fragment_Stage_0202_Item_00()
    If MTNS06_Uranium_PA_BossSpawn && !MTNS06_Uranium_PA_BossSpawn.IsPlaying()
        MTNS06_Uranium_PA_BossSpawn.Start()
    EndIf
EndFunction

Function Fragment_Stage_0203_Item_00()
    If MTNS06_Uranium_PA_BossSpawn && !MTNS06_Uranium_PA_BossSpawn.IsPlaying()
        MTNS06_Uranium_PA_BossSpawn.Start()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    If MTNS06_Uranium_PA_ActivityEnd && !MTNS06_Uranium_PA_ActivityEnd.IsPlaying()
        MTNS06_Uranium_PA_ActivityEnd.Start()
    EndIf
EndFunction

Function Fragment_Stage_0325_Item_00()
    If MTNS06_Uranium_PA_ActivityEnd && !MTNS06_Uranium_PA_ActivityEnd.IsPlaying()
        MTNS06_Uranium_PA_ActivityEnd.Start()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    Stop()
EndFunction
