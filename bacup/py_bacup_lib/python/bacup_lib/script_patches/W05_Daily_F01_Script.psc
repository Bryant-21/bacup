Event OnQuestInit()
    Int variantCount = Maguffins.Length
    If variantCount > 0
        Int selectedIndex = Utility.RandomInt(0, variantCount - 1)
        SetStage(Maguffins[selectedIndex].stageID)
    EndIf

    Actor thief = alias_Thief.GetActorReference()
    ObjectReference player = alias_Player.GetReference()
    If thief != None && player != None
        RegisterForDistanceLessThanEvent(player, thief, MaxDistance as Float)
    EndIf
EndEvent

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    If W05_Daily_F01_SignalStrengthMessage != None
        W05_Daily_F01_SignalStrengthMessage.Show(afDistance)
    EndIf
    SetStage(300)
EndEvent
