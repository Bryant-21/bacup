Function Fragment_Phase_01_Begin()
    EN07_PosterTutorialScript tutorial = GetOwningQuest() as EN07_PosterTutorialScript
    If tutorial != None
        tutorial.EN07Tutorial_SetLight(1)
    EndIf
EndFunction

Function Fragment_Phase_02_Begin()
    EN07_PosterTutorialScript tutorial = GetOwningQuest() as EN07_PosterTutorialScript
    If tutorial != None
        tutorial.EN07Tutorial_SetLight(2)
    EndIf
EndFunction

Function Fragment_Phase_03_Begin()
    EN07_PosterTutorialScript tutorial = GetOwningQuest() as EN07_PosterTutorialScript
    If tutorial != None
        tutorial.EN07Tutorial_SetLight(3)
    EndIf
EndFunction
