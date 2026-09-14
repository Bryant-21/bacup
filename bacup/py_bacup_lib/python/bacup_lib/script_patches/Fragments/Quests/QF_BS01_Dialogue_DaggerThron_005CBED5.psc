Function Fragment_Stage_0100_Item_00()
    Actor daggerRef = Alias_Dagger.GetActorReference()
    If daggerRef && daggerRef.IsDead()
        daggerRef.Disable()
    EndIf
EndFunction
