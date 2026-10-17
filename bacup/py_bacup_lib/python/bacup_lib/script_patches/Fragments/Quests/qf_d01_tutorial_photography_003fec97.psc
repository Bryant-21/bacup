Function Fragment_Stage_0100_Item_00()
    Actor playerActor = Alias_Player.GetActorReference()
    ; The fragment VMAD binds this value as PD01_Photography_Tutorial_Craft, so the skeleton's property is None.
    If playerActor != None && D01_Photography_Tutorial_Craft != None
        playerActor.SetValue(D01_Photography_Tutorial_Craft, 1.0)
    EndIf
    D01_Tutorial_Photography01.Show()
    D01_Tutorial_Photography02.Show()
    D01_Tutorial_Photography03.Show()
    Stop()
EndFunction
