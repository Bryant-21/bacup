Function Fragment_Phase_01_Begin()
    EN07_FleeSiloScript controller = GetOwningQuest() as EN07_FleeSiloScript
    If controller != None
        controller.SetLaunchSoundPhase(1)
    EndIf
EndFunction

Function Fragment_Phase_07_Begin()
    EN07_FleeSiloScript controller = GetOwningQuest() as EN07_FleeSiloScript
    If controller != None
        controller.SetLaunchSoundPhase(2)
    EndIf
EndFunction
