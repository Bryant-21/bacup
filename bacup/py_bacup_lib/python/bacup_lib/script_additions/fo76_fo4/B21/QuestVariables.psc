Scriptname B21:QuestVariables Extends Quest
{FO4 display backing for FO76 quest variables.

FO76 objective and log text substitutes <Variable=Name> from per-quest
variables. The converter creates one GLOB per variable, lists it in the quest's
text display globals and rewrites the text to <Global=EditorID>. Scripts update
the displayed value through SetVariable.}

String[] Property VariableNames Auto Const
GlobalVariable[] Property VariableGlobals Auto Const

GlobalVariable Function FindVariableGlobal(String asName)
    If VariableNames == None || VariableGlobals == None
        Return None
    EndIf
    Int index = VariableNames.Find(asName)
    If index < 0 || index >= VariableGlobals.Length
        Return None
    EndIf
    Return VariableGlobals[index]
EndFunction

Function SetVariable(String asName, Float afValue)
    GlobalVariable variableGlobal = FindVariableGlobal(asName)
    If variableGlobal == None
        Return
    EndIf
    variableGlobal.SetValue(afValue)
    UpdateCurrentInstanceGlobal(variableGlobal)
EndFunction

Float Function GetVariable(String asName)
    GlobalVariable variableGlobal = FindVariableGlobal(asName)
    If variableGlobal == None
        Return 0.0
    EndIf
    Return variableGlobal.GetValue()
EndFunction
