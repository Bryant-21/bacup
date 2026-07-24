Function Fragment_Stage_0010_Item_00()
    If Alias_ClutterMarkerEnable.GetReference() != None
        Alias_ClutterMarkerEnable.GetReference().Enable()
    EndIf
    If Alias_ClutterMarkerDisable.GetReference() != None
        Alias_ClutterMarkerDisable.GetReference().Disable()
    EndIf
EndFunction

Function Fragment_Stage_0015_Item_00()
    If W05_RE_CampAF02_DoctorAnims != None
        W05_RE_CampAF02_DoctorAnims.Start()
    EndIf
    Actor patientActor = Alias_Patient02.GetActorReference()
    If patientActor != None && Health != None
        patientActor.DamageValue(Health, patientActor.GetValue(Health))
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
EndFunction
