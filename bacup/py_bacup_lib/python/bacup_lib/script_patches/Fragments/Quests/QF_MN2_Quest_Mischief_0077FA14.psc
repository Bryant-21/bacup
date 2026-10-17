; MN2_QuestScript owns the Mischief Meter, waves and failsafes; these fragments own objectives and the pyres.
; Pyres 1-3 stay disabled until the previous area's meter is full: their DefaultAliasOnActivate has no prereq stage.
MN2_QuestScript Function Controller()
    Return (Self as Quest) as MN2_QuestScript
EndFunction

Function SetPyre(ReferenceAlias akPyre, Bool abEnabled, Bool abLit)
    If akPyre == None || akPyre.GetReference() == None
        Return
    EndIf
    ObjectReference pyre = akPyre.GetReference()
    If abEnabled
        pyre.Enable()
    Else
        pyre.Disable()
    EndIf
    MN2_BonfireScript fire = pyre as MN2_BonfireScript
    If fire != None
        fire.ToggleBonfire(abLit)
    EndIf
EndFunction

Function ShowMayhemTasks(Bool abShow)
    SetObjectiveDisplayed(50, abShow)
    SetObjectiveDisplayed(60, abShow)
    SetObjectiveDisplayed(70, abShow)
    SetObjectiveDisplayed(80, abShow)
    SetObjectiveDisplayed(90, abShow)
    SetObjectiveDisplayed(100, abShow)
EndFunction

Function LightPyre(ReferenceAlias akPyre)
    SetPyre(akPyre, True, True)
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True, True)
EndFunction

Function MayhemStarted(Int aiObjective, Message akAreaMessage)
    SetObjectiveCompleted(20, True)
    If akAreaMessage != None
        akAreaMessage.Show()
    EndIf
    SetObjectiveDisplayed(aiObjective, True)
    ShowMayhemTasks(True)
    SetObjectiveDisplayed(120, True)
EndFunction

Function MayhemDone(Int aiObjective, ReferenceAlias akNextPyre)
    SetObjectiveCompleted(aiObjective, True)
    ShowMayhemTasks(False)
    SetPyre(akNextPyre, True, False)
    SetObjectiveCompleted(10, False)
    SetObjectiveCompleted(20, False)
    SetObjectiveDisplayed(10, True, True)
    If MN2_Mischief_LightPyreMessage != None
        MN2_Mischief_LightPyreMessage.Show()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetPyre(Alias_Activator_Pyre00, True, False)
    SetPyre(Alias_Activator_Pyre01, False, False)
    SetPyre(Alias_Activator_Pyre02, False, False)
    SetPyre(Alias_Activator_Pyre03, False, False)
    MN2_QuestScript ctrl = Controller()
    If ctrl != None
        ctrl.StartEvent()
    EndIf
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    LightPyre(Alias_Activator_Pyre00)
EndFunction

Function Fragment_Stage_0270_Item_00()
    MayhemStarted(30, MN2_Mischief_ParkMessage)
EndFunction

Function Fragment_Stage_0290_Item_00()
    MayhemDone(30, Alias_Activator_Pyre01)
EndFunction

Function Fragment_Stage_0300_Item_00()
    LightPyre(Alias_Activator_Pyre01)
EndFunction

Function Fragment_Stage_0370_Item_00()
    MayhemStarted(32, MN2_Mischief_CottagesMessage)
EndFunction

Function Fragment_Stage_0390_Item_00()
    MayhemDone(32, Alias_Activator_Pyre02)
EndFunction

Function Fragment_Stage_0400_Item_00()
    LightPyre(Alias_Activator_Pyre02)
EndFunction

Function Fragment_Stage_0470_Item_00()
    MayhemStarted(34, MN2_Mischief_CourtyardMessage)
EndFunction

Function Fragment_Stage_0490_Item_00()
    MayhemDone(34, Alias_Activator_Pyre03)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetPyre(Alias_Activator_Pyre03, True, True)
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(40, True)
EndFunction

Function Fragment_Stage_0520_Item_00()
    SetObjectiveCompleted(40, True)
    SetObjectiveDisplayed(110, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9990_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9991_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9992_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_10000_Item_00()
    SetPyre(Alias_Activator_Pyre00, True, False)
    SetPyre(Alias_Activator_Pyre01, True, False)
    SetPyre(Alias_Activator_Pyre02, True, False)
    SetPyre(Alias_Activator_Pyre03, True, False)
    If Alias_Statics_PaintSplats != None
        Alias_Statics_PaintSplats.DisableAll()
    EndIf
EndFunction
