Function Fragment_Begin(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None
        Game.GetPlayer().SetValue(BS02_AV_FlirtedWithShin, 1.0)
    EndIf
EndFunction
