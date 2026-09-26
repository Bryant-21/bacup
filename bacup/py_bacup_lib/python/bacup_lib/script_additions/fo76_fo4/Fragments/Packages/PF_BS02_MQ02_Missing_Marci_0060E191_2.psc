Scriptname Fragments:Packages:PF_BS02_MQ02_Missing_Marci_0060E191_2 Extends Package Hidden Const

Function Fragment_End(Actor akActor)
    Quest host = GetOwningQuest()
    If host == None || akActor == None || !host.IsRunning() || host.IsCompleted()
        Return
    EndIf
    If !host.IsStageDone(1000) || host.IsStageDone(1025) || host.IsStageDone(1050)
        Return
    EndIf
    ReferenceAlias marcia = host.GetAlias(10) as ReferenceAlias
    If marcia != None && akActor == marcia.GetActorReference()
        host.SetStage(1025)
    EndIf
EndFunction
