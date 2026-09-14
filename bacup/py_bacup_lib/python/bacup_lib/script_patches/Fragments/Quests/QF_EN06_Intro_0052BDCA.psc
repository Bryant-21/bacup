Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0020_Item_00()
    If EN06_Intro_0020_MODUSIntro != None && !EN06_Intro_0020_MODUSIntro.IsPlaying()
        EN06_Intro_0020_MODUSIntro.Start()
    EndIf
EndFunction
